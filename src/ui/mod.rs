mod dialogs;
mod views;
use crate::{codec,model::*,redis::{Cancellation,Client,command,parse_command},service::{Service,ScanState,SlowLog,info_value},storage::{Store,flatten}};
use anyhow::Result;
use gpui::*;
use gpui_component::{button::*,input::{Input,InputEvent,InputState},menu::{DropdownMenu,PopupMenuItem,ContextMenuExt},resizable::*,*};
use parking_lot::Mutex;
use serde_json::Value as Json;
use std::{borrow::Cow,collections::{BTreeMap,HashSet,VecDeque},path::PathBuf,sync::{Arc,mpsc},time::{Duration,Instant}};

#[derive(rust_embed::RustEmbed)]
#[folder="assets/"]
struct Assets;
impl AssetSource for Assets {
    fn load(&self,path:&str)->Result<Option<Cow<'static,[u8]>>> {if let Some(f)=Self::get(path){Ok(Some(f.data))}else{gpui_component_assets::Assets.load(path)}}
    fn list(&self,path:&str)->Result<Vec<SharedString>> {let mut a=gpui_component_assets::Assets.list(path)?;a.extend(Self::iter().filter(|p|p.starts_with(path)).map(|p|SharedString::from(p.into_owned())));Ok(a)}
}
actions!(rdm,[NewConnection,PreferencesAction,Refresh,SaveValue,CloseTab,CancelDialog,NewKey,FocusFilter]);
pub fn run() {
    let store=Store::system();let pref=store.preferences().unwrap_or_default();
    let bounds=size(px(pref.behavior.window_width.max(960.)),px(pref.behavior.window_height.max(640.)));
    Application::new().with_assets(Assets).run(move |cx| {
        gpui_component::init(cx);
        cx.bind_keys([
            KeyBinding::new("cmd-n",NewConnection,Some("Rdm")),KeyBinding::new("ctrl-n",NewConnection,Some("Rdm")),
            KeyBinding::new("cmd-,",PreferencesAction,Some("Rdm")),KeyBinding::new("ctrl-,",PreferencesAction,Some("Rdm")),
            KeyBinding::new("cmd-r",Refresh,Some("Rdm")),KeyBinding::new("f5",Refresh,Some("Rdm")),
            KeyBinding::new("cmd-s",SaveValue,Some("Rdm")),KeyBinding::new("ctrl-s",SaveValue,Some("Rdm")),
            KeyBinding::new("cmd-w",CloseTab,Some("Rdm")),KeyBinding::new("ctrl-w",CloseTab,Some("Rdm")),
            KeyBinding::new("escape",CancelDialog,Some("Rdm")),KeyBinding::new("cmd-f",FocusFilter,Some("Rdm")),
        ]);
        cx.open_window(WindowOptions {
            window_bounds:Some(WindowBounds::Windowed(Bounds::centered(None,bounds,cx))),window_min_size:Some(size(px(960.),px(640.))),
            titlebar:Some(TitlebarOptions {title:Some("Tiny RDM".into()),appears_transparent:true,traffic_light_position:Some(point(px(12.),px(12.))),..Default::default()}),..Default::default()
        },move |window,cx| {let view=cx.new(|cx|RdmApp::new(store,window,cx));cx.new(|cx|Root::new(view,window,cx))}).expect("无法创建窗口");cx.activate(true);
    });
}
#[derive(Clone,Copy,PartialEq,Eq)] enum Nav {Browser,Server,Log}
#[derive(Clone,Copy,PartialEq,Eq)] enum Pane {Status,Detail,Cli,Slow,Monitor,Pubsub}
struct StreamState {cancel:Cancellation,receiver:mpsc::Receiver<String>,lines:VecDeque<String>,paused:bool}
impl Drop for StreamState {fn drop(&mut self){self.cancel.cancel();}}
struct Sample {time:String,ops:f64,clients:f64,memory:f64,input:f64,output:f64}
struct Tab {
    id:u64,conn:Connection,db:u32,pane:Pane,databases:Vec<Database>,info:Info,samples:VecDeque<Sample>,
    keys:Vec<KeyEntry>,scan:ScanState,total:u64,selected:HashSet<Vec<u8>>,check_mode:bool,collapsed:HashSet<String>,flat:bool,
    filter:Entity<InputState>,key_type:String,exact:bool,detail:Option<KeyDetail>,current_key:Option<Vec<u8>>,detail_epoch:u64,
    editor:Entity<InputState>,row_filter:Entity<InputState>,format:String,decode:String,reverse:bool,editing:bool,auto_refresh:bool,
    cli_input:Entity<InputState>,cli_output:String,cli_history:Vec<String>,cli_history_index:usize,cli_db:u32,
    slow:Vec<SlowLog>,monitor:Option<StreamState>,pubsub:Option<StreamState>,
    stream_filter:Entity<InputState>,pub_channel:Entity<InputState>,pub_message:Entity<InputState>,pub_pattern:Entity<InputState>,
}
struct RdmApp {
    store:Store,service:Arc<Mutex<Service>>,connections:Vec<Connection>,pref:Preferences,lang:Json,nav:Nav,
    tabs:Vec<Tab>,active:Option<u64>,next_id:u64,dialog:Option<dialogs::Dialog>,
    conn_filter:Entity<InputState>,log_filter:Entity<InputState>,focus:FocusHandle,subs:Vec<Subscription>,
    busy:usize,toast:Option<(String,bool,Instant)>,last_poll:Instant,last_auto:Instant,log_page:usize,
    resizable:Entity<ResizableState>,transfer:Option<Cancellation>,full_editor:bool,info_details:bool,
}
fn input(text:impl Into<SharedString>,placeholder:impl Into<SharedString>,multi:bool,window:&mut Window,cx:&mut App)->Entity<InputState> {
    let text=text.into();let placeholder=placeholder.into();cx.new(|cx|{let mut i=InputState::new(window,cx).placeholder(placeholder).multi_line(multi);i.set_value(text,window,cx);i})
}
fn set_input(state:&Entity<InputState>,text:impl Into<SharedString>,window:&mut Window,cx:&mut App) {let text=text.into();state.update(cx,|i,cx|i.set_value(text,window,cx));}
fn text_input(state:&Entity<InputState>,cx:&App)->String {state.read(cx).value().to_string()}
fn icon(name:&str)->Icon {Icon::new(SharedString::from(format!("icons/{name}.svg")))}
fn human_bytes(n:f64)->String {if n>=1024.*1024.*1024. {format!("{:.2} GB",n/1024./1024./1024.)} else if n>=1024.*1024. {format!("{:.2} MB",n/1024./1024.)}else if n>=1024. {format!("{:.2} KB",n/1024.)} else {format!("{n:.0} B")} }
impl RdmApp {
    fn new(store:Store,window:&mut Window,cx:&mut Context<Self>)->Self {
        let connections=store.connections();let preferences=store.preferences();let pref=preferences.as_ref().cloned().unwrap_or_default();
        let mut app=Self {service:Arc::new(Mutex::new(Service::new(store.clone()))),store,connections:connections.as_ref().cloned().unwrap_or_default(),pref,
            lang:Json::Null,nav:Nav::Server,tabs:vec![],active:None,next_id:0,dialog:None,
            conn_filter:input("","筛选",false,window,cx),log_filter:input("","筛选",false,window,cx),focus:cx.focus_handle(),subs:vec![],
            busy:0,toast:None,last_poll:Instant::now(),last_auto:Instant::now(),log_page:0,resizable:cx.new(|_|ResizableState::default()),transfer:None,full_editor:false,info_details:false};
        if let Err(e)=connections {app.toast(e.to_string(),true);}if let Err(e)=preferences {app.toast(e.to_string(),true);}
        app.apply_theme(window,cx);app.load_language();
        cx.spawn_in(window,async move |this,cx| {loop {cx.background_executor().timer(Duration::from_millis(150)).await;if this.update_in(cx,|app,window,cx|app.tick(window,cx)).is_err(){break;}}}).detach();app
    }
    fn load_language(&mut self) {
        let code=if self.pref.general.language=="auto" {std::env::var("RUST_RDM_LANGUAGE").unwrap_or_else(|_|if std::env::var("LANG").unwrap_or_default().starts_with("zh"){"zh-cn".into()}else{"en-us".into()})}else{self.pref.general.language.clone()};
        let code=match code.as_str(){"zh"=>"zh-cn","en"=>"en-us","tw"=>"zh-tw","ja"=>"ja-jp","ko"=>"ko-kr","fr"=>"fr-fr","es"=>"es-es","pt"=>"pt-br","ru"=>"ru-ru","tr"=>"tr-tr",_=>&code};
        self.lang=Assets::get(&format!("langs/{code}.json")).and_then(|f|serde_json::from_slice(&f.data).ok()).unwrap_or_else(||serde_json::from_slice(&Assets::get("langs/en-us.json").unwrap().data).unwrap());
    }
    fn tr(&self,key:&str)->String {self.lang.pointer(&format!("/{}",key.replace('.','/'))).and_then(Json::as_str).unwrap_or(key).into()}
    fn apply_theme(&self,window:&mut Window,cx:&mut App) {
        let mode=match self.pref.general.theme.as_str(){"dark"=>ThemeMode::Dark,"light"=>ThemeMode::Light,_=>match window.appearance(){WindowAppearance::Dark|WindowAppearance::VibrantDark=>ThemeMode::Dark,_=>ThemeMode::Light}};
        Theme::change(mode,Some(window),cx);let t=Theme::global_mut(cx);t.font_size=px(self.pref.general.font_size);t.radius=px(4.);t.primary=rgb(0xd33a31).into();t.primary_foreground=rgb(0xffffff).into();t.ring=rgb(0xd33a31).into();
        t.background=rgb(if mode==ThemeMode::Dark{0x1e1e1e}else{0xffffff}).into();t.border=rgb(if mode==ThemeMode::Dark{0x515151}else{0xe0e0e6}).into();if !self.pref.general.font.is_empty(){t.font_family=self.pref.general.font.clone().into();}
    }
    fn dark(&self,cx:&App)->bool {cx.theme().mode==ThemeMode::Dark}
    fn sidebar_color(&self,cx:&App)->Hsla {rgb(if self.dark(cx){0x262626}else{0xf2f2f2}).into()}
    fn ribbon_color(&self,cx:&App)->Hsla {rgb(if self.dark(cx){0x2c2c2c}else{0xf9f9f9}).into()}
    fn toast(&mut self,text:impl Into<String>,error:bool) {self.toast=Some((text.into(),error,Instant::now()));}
    fn save_preferences(&mut self) {if let Err(e)=self.store.save("preferences.yaml",&self.pref){self.toast(e.to_string(),true);}}
    fn save_connections(&mut self) {if let Err(e)=self.store.save("connections.yaml",&self.connections){self.toast(e.to_string(),true);}}
    fn index(&self)->Option<usize> {let id=self.active?;self.tabs.iter().position(|t|t.id==id)}
    fn tab_index(&self,id:u64)->Option<usize>{self.tabs.iter().position(|t|t.id==id)}
    fn job<T:Send+'static>(&mut self,window:&mut Window,cx:&mut Context<Self>,work:impl FnOnce(&mut Service)->Result<T>+Send+'static,apply:impl FnOnce(&mut Self,T,&mut Window,&mut Context<Self>)+'static) {
        self.busy+=1;let service=self.service.clone();let task=cx.background_executor().spawn(async move {work(&mut service.lock())});
        cx.spawn_in(window,async move |this,cx| {let result=task.await;let _=this.update_in(cx,|app,window,cx|{app.busy=app.busy.saturating_sub(1);match result{Ok(value)=>apply(app,value,window,cx),Err(e)=>app.toast(e.to_string(),true)}cx.notify();});}).detach();cx.notify();
    }
    fn open_connection(&mut self,c:Connection,window:&mut Window,cx:&mut Context<Self>) {
        if let Some(t)=self.tabs.iter().find(|t|t.conn.name==c.name){self.active=Some(t.id);self.nav=Nav::Browser;cx.notify();return;}
        self.next_id+=1;let id=self.next_id;let db=if c.cluster.enable{0}else{c.last_db};let line_number=self.pref.editor.show_line_num;
        let editor=cx.new(|cx|InputState::new(window,cx).code_editor("json").line_number(line_number));
        let filter=input(c.default_filter.clone(),self.tr("interface.filter"),false,window,cx);let row_filter=input("",self.tr("interface.search"),false,window,cx);let cli_input=input("",">",false,window,cx);
        for (field,state) in [("filter",filter.clone()),("row",row_filter.clone()),("cli",cli_input.clone())] {self.subs.push(cx.subscribe_in(&state,window,move|app,_,event,window,cx|{if matches!(event,InputEvent::PressEnter{..}){match field{"filter"=>app.scan_keys(id,true,false,window,cx),"row"=>app.reload_detail(id,true,window,cx),_=>app.execute_cli(id,window,cx)}}}));}
        let tab=Tab{id,conn:c.clone(),db,pane:Pane::Status,databases:vec![],info:Info::default(),samples:VecDeque::new(),keys:vec![],scan:ScanState::default(),total:0,selected:HashSet::new(),check_mode:false,collapsed:HashSet::new(),flat:c.key_view==1,
            filter,key_type:String::new(),exact:false,detail:None,current_key:None,detail_epoch:0,editor,row_filter,format:"Auto".into(),decode:"Auto".into(),reverse:false,editing:false,auto_refresh:false,
            cli_input,cli_output:format!("{}\n",self.tr("interface.cli_welcome")),cli_history:vec![],cli_history_index:0,cli_db:db,slow:vec![],monitor:None,pubsub:None,
            stream_filter:input("","筛选",false,window,cx),pub_channel:input("","Channel",false,window,cx),pub_message:input("","Message",true,window,cx),pub_pattern:input("*","Pattern",false,window,cx)};
        self.tabs.push(tab);self.active=Some(id);self.nav=Nav::Browser;
        self.job(window,cx,move|s|{s.connect(&c,db)?;Ok((s.databases(&c)?,s.info(&c)?))},move|app,(databases,info),window,cx|{if let Some(i)=app.tab_index(id){app.tabs[i].databases=databases;app.tabs[i].info=info;app.scan_keys(id,true,false,window,cx);}});
    }
    fn close_tab(&mut self,id:u64,window:&mut Window,cx:&mut Context<Self>) {
        let Some(i)=self.tab_index(id)else{return;};let name=self.tabs[i].conn.name.clone();self.tabs.remove(i);if self.active==Some(id){self.active=self.tabs.last().map(|t|t.id);}if self.tabs.is_empty(){self.nav=Nav::Server;}
        self.job(window,cx,move|s|{s.close(&name);s.close(&format!("cli:{name}"));Ok(())},|_,_,_,_|{});
    }
    fn scan_keys(&mut self,id:u64,reset:bool,all:bool,window:&mut Window,cx:&mut Context<Self>) {
        let Some(i)=self.tab_index(id)else{return;};let t=&mut self.tabs[i];if reset{t.scan=ScanState::default();t.keys.clear();t.selected.clear();}
        let c=t.conn.clone();let db=t.db;let state=t.scan.clone();let pattern=text_input(&t.filter,cx);let kind=t.key_type.clone();let exact=t.exact;let count=self.pref.general.scan_size;
        self.job(window,cx,move|s|{let mut page=s.scan(&c,db,&pattern,&kind,exact,state,count)?;if all{while !page.state.end{let next=s.scan(&c,db,&pattern,&kind,exact,page.state.clone(),count)?;page.keys.extend(next.keys);page.state=next.state;}}Ok(page)},move|app,page,_,_|{if let Some(i)=app.tab_index(id)&&app.tabs[i].db==db{let t=&mut app.tabs[i];let mut seen=t.keys.iter().map(|k|k.key.clone()).collect::<HashSet<_>>();t.keys.extend(page.keys.into_iter().filter(|k|seen.insert(k.key.clone())));t.keys.sort_by(|a,b|a.key.cmp(&b.key));t.scan=page.state;t.total=page.total;}});
    }
    fn change_database(&mut self,id:u64,db:u32,window:&mut Window,cx:&mut Context<Self>) {if let Some(i)=self.tab_index(id){let t=&mut self.tabs[i];t.db=db;t.detail=None;t.current_key=None;t.detail_epoch+=1;t.conn.last_db=db;let name=t.conn.name.clone();let c=t.conn.clone();let _=crate::storage::save_connection(&mut self.connections,Some(&name),c);self.save_connections();self.scan_keys(id,true,false,window,cx);}}
    fn select_key(&mut self,id:u64,key:Vec<u8>,window:&mut Window,cx:&mut Context<Self>) {if let Some(i)=self.tab_index(id){let t=&mut self.tabs[i];if t.check_mode{if !t.selected.insert(key.clone()){t.selected.remove(&key);}cx.notify();return;}t.pane=Pane::Detail;t.current_key=Some(key);t.detail=None;t.decode="Auto".into();t.format="Auto".into();t.auto_refresh=false;self.reload_detail(id,true,window,cx);}}
    fn reload_detail(&mut self,id:u64,reset:bool,window:&mut Window,cx:&mut Context<Self>) {
        let Some(i)=self.tab_index(id)else{return;};let t=&mut self.tabs[i];let Some(key)=t.current_key.clone()else{return;};t.detail_epoch+=1;let epoch=t.detail_epoch;let c=t.conn.clone();let db=t.db;
        let cursor=if reset{0}else{t.detail.as_ref().map(|d|d.cursor).unwrap_or(0)};let after=if reset{String::new()}else{t.detail.as_ref().and_then(|d|d.rows.last()).map(|r|display_bytes(&r.id)).unwrap_or_default()};let pattern=text_input(&t.row_filter,cx);let reverse=t.reverse;
        self.job(window,cx,move|s|s.detail(&c,db,&key,cursor,&pattern,100,&after,reverse),move|app,mut detail,window,cx|{
            if let Some(i)=app.tab_index(id)&&app.tabs[i].detail_epoch==epoch{
                let t=&mut app.tabs[i];if !reset&&let Some(old)=&t.detail{let mut rows=old.rows.clone();let mut seen=rows.iter().map(|r|r.id.clone()).collect::<HashSet<_>>();rows.extend(detail.rows.into_iter().filter(|r|seen.insert(r.id.clone())));detail.rows=rows;}
                if matches!(detail.kind,KeyType::String|KeyType::Json){let result=codec::convert(&detail.value,&t.decode,&t.format,&app.pref.decoder);match result{Ok(v)=>{t.decode=v.decode;t.format=v.format;set_input(&t.editor,v.text,window,cx);},Err(e)=>{app.toast(e.to_string(),true);}}}app.tabs[i].detail=Some(detail);app.tabs[i].editing=false;
            }
        });
    }
    fn save_value(&mut self,window:&mut Window,cx:&mut Context<Self>) {
        let Some(i)=self.index()else{return;};let t=&self.tabs[i];let Some(d)=&t.detail else{return;};let c=t.conn.clone();let db=t.db;let id=t.id;let key=d.key.clone();let json=d.kind==KeyType::Json;
        let value=match codec::encode(&text_input(&t.editor,cx),&t.decode,&t.format,&self.pref.decoder){Ok(v)=>v,Err(e)=>{self.toast(e.to_string(),true);return;}};
        self.job(window,cx,move|s|s.set_string(&c,db,&key,value,json),move|app,_,window,cx|{app.toast(app.tr("interface.save_value_succ"),false);app.reload_detail(id,true,window,cx);});
    }
    fn refresh_info(&mut self,id:u64,window:&mut Window,cx:&mut Context<Self>) {let Some(i)=self.tab_index(id)else{return;};let c=self.tabs[i].conn.clone();self.job(window,cx,move|s|s.info(&c),move|app,info,_,_|{if let Some(i)=app.tab_index(id){let f=|section,key|info_value(&info,section,key).parse::<f64>().unwrap_or_default();let sample=Sample{time:chrono::Local::now().format("%H:%M:%S").to_string(),ops:f("Stats","instantaneous_ops_per_sec"),clients:f("Clients","connected_clients"),memory:f("Memory","used_memory"),input:f("Stats","instantaneous_input_kbps")*1024.,output:f("Stats","instantaneous_output_kbps")*1024.};let t=&mut app.tabs[i];t.info=info;t.samples.push_back(sample);while t.samples.len()>60{t.samples.pop_front();}}});}
    fn switch_pane(&mut self,id:u64,pane:Pane,window:&mut Window,cx:&mut Context<Self>) {let Some(i)=self.tab_index(id)else{return;};self.tabs[i].pane=pane;match pane{Pane::Status=>self.refresh_info(id,window,cx),Pane::Slow=>self.load_slow(id,window,cx),_=>{}}cx.notify();}
    fn load_slow(&mut self,id:u64,window:&mut Window,cx:&mut Context<Self>){let Some(i)=self.tab_index(id)else{return;};let c=self.tabs[i].conn.clone();self.job(window,cx,move|s|s.slow_logs(&c,128),move|app,logs,_,_|{if let Some(i)=app.tab_index(id){app.tabs[i].slow=logs;}});}
    fn execute_cli(&mut self,id:u64,window:&mut Window,cx:&mut Context<Self>) {
        let Some(i)=self.tab_index(id)else{return;};let t=&mut self.tabs[i];let line=text_input(&t.cli_input,cx);if line.trim().is_empty(){return;}
        let args=match parse_command(&line){Ok(a)=>a,Err(e)=>{self.toast(e.to_string(),true);return;}};let mut c=t.conn.clone();c.name=format!("cli:{}",c.name);let db=t.conn.last_db;
        if line.eq_ignore_ascii_case("clear"){t.cli_output.clear();set_input(&t.cli_input,"",window,cx);cx.notify();return;}
        t.cli_history.push(line.clone());t.cli_history_index=t.cli_history.len();t.cli_output.push_str(&format!("{}:{}[{}]> {line}\n",c.addr,c.port,t.cli_db));set_input(&t.cli_input,"",window,cx);
        if args.first().is_some_and(|a|a.eq_ignore_ascii_case(b"SELECT"))&&let Some(n)=args.get(1).and_then(|n|std::str::from_utf8(n).ok()).and_then(|s|s.parse().ok()){t.cli_db=n;}
        self.job(window,cx,move|s|Ok(s.run(&c,db,args).map(|v|v.text()).unwrap_or_else(|e|format!("(error) {e}"))),move|app,text,_,_|{if let Some(i)=app.tab_index(id){app.tabs[i].cli_output.push_str(&text);app.tabs[i].cli_output.push('\n');}});
    }
    fn stream_toggle(&mut self,id:u64,pubsub:bool,_window:&mut Window,cx:&mut Context<Self>) {
        let Some(i)=self.tab_index(id)else{return;};let t=&mut self.tabs[i];let current=if pubsub{&mut t.pubsub}else{&mut t.monitor};if current.is_some(){*current=None;cx.notify();return;}
        let mut c=t.conn.clone();c.exec_timeout=1;let db=t.db;let pattern=text_input(&t.pub_pattern,cx);let cancel=Cancellation::default();let token=cancel.clone();let(sender,receiver)=mpsc::sync_channel(1024);*current=Some(StreamState{cancel,receiver,lines:VecDeque::new(),paused:false});
        std::thread::spawn(move||{let result=(||->Result<()>{let client=Client::connect(&c,db)?;let mut wire=client.dedicated()?;if pubsub{wire.send(&command(&["PSUBSCRIBE",if pattern.is_empty(){"*"}else{&pattern}]))?;}else{wire.query(&command(&["MONITOR"]))?.checked()?;}while !token.canceled(){match wire.read(){Ok(v)=>{let _=sender.try_send(format!("{} {}",chrono::Local::now().format("%H:%M:%S%.3f"),v.text()));},Err(e)if crate::redis::is_timeout(&e)=>{},Err(e)=>return Err(e)}}Ok(())})();if let Err(e)=result{let _=sender.try_send(format!("(error) {e}"));}});cx.notify();
    }
    fn publish(&mut self,id:u64,window:&mut Window,cx:&mut Context<Self>) {let Some(i)=self.tab_index(id)else{return;};let t=&self.tabs[i];let channel=text_input(&t.pub_channel,cx);let message=text_input(&t.pub_message,cx);let c=t.conn.clone();let db=t.db;self.job(window,cx,move|s|s.run(&c,db,command(&["PUBLISH",&channel,&message])),|app,value,_,_|app.toast(format!("{} subscribers",value.text()),false));}
    fn tick(&mut self,window:&mut Window,cx:&mut Context<Self>) {
        let mut changed=false;for tab in &mut self.tabs{for stream in [&mut tab.monitor,&mut tab.pubsub].into_iter().flatten(){while let Ok(line)=stream.receiver.try_recv(){if !stream.paused{stream.lines.push_back(line);changed=true;while stream.lines.len()>10000{stream.lines.pop_front();}}}}}
        if self.toast.as_ref().is_some_and(|(_,_,time)|time.elapsed()>Duration::from_secs(6)){self.toast=None;changed=true;}
        if self.last_poll.elapsed()>Duration::from_secs(5)&&self.busy==0{self.last_poll=Instant::now();if let Some(i)=self.index()&&self.nav==Nav::Browser&&self.tabs[i].pane==Pane::Status{self.refresh_info(self.tabs[i].id,window,cx);}}
        if self.last_auto.elapsed()>Duration::from_secs(2)&&self.busy==0{self.last_auto=Instant::now();if let Some(i)=self.index()&&self.tabs[i].auto_refresh&&self.tabs[i].pane==Pane::Detail{self.reload_detail(self.tabs[i].id,true,window,cx);}}if changed{cx.notify();}
    }
}
impl Render for RdmApp {fn render(&mut self,window:&mut Window,cx:&mut Context<Self>)->impl IntoElement {self.render_app(window,cx)}}
