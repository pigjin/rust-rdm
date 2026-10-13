use super::*;
use super::dialogs::DialogKind;
use gpui::prelude::*;
use gpui_component::{chart::AreaChart,checkbox::Checkbox};

pub(super) fn menu_action(label:impl Into<SharedString>,weak:WeakEntity<RdmApp>,handler:impl Fn(&mut RdmApp,&mut Window,&mut Context<RdmApp>)+'static)->PopupMenuItem {
    PopupMenuItem::new(label).on_click(move|_,window,cx|{let _=weak.update(cx,|app,cx|handler(app,window,cx));})
}
impl RdmApp {
    pub(super) fn btn(&self,id:impl Into<SharedString>,label:impl Into<SharedString>,ico:&str,cx:&Context<Self>,handler:impl Fn(&mut Self,&mut Window,&mut Context<Self>)+'static)->Button {
        let mut b=Button::new(id.into()).label(label).on_click(cx.listener(move|app,_,w,cx|handler(app,w,cx))).h(px(32.));if !ico.is_empty(){b=b.icon(icon(ico).size(px(18.)));}b
    }
    pub(super) fn icon_btn(&self,id:impl Into<SharedString>,ico:&str,tip:impl Into<SharedString>,cx:&Context<Self>,handler:impl Fn(&mut Self,&mut Window,&mut Context<Self>)+'static)->Button {
        self.btn(id,"",ico,cx,handler).ghost().compact().tooltip(tip).w(px(28.))
    }
    pub(super) fn render_app(&mut self,window:&mut Window,cx:&mut Context<Self>)->AnyElement {
        let title=self.render_title(cx);let ribbon=self.render_ribbon(cx);
        let content=if self.nav==Nav::Log{self.render_log(cx)}else{
            let sidebar=if self.nav==Nav::Server{self.render_connections(cx)}else{self.render_browser(cx)};
            let main=if self.nav==Nav::Server{self.render_empty(cx)}else{self.render_content(window,cx)};
            let weak=cx.weak_entity();
            h_resizable("rdm-panels").with_state(&self.resizable)
                .child(resizable_panel().size(px(self.pref.behavior.aside_width)).size_range(px(300.)..px(700.)).child(sidebar))
                .child(resizable_panel().child(main))
                .on_resize(move|state,_,cx|{let size=state.read(cx).sizes().first().copied();if let Some(s)=size{let _=weak.update(cx,|app,_|{app.pref.behavior.aside_width=s.as_f32();app.save_preferences();});}}).into_any_element()
        };
        let mut root=div().id("rdm-root").key_context("Rdm").track_focus(&self.focus).size_full().relative().flex().flex_col().overflow_hidden().bg(cx.theme().background).text_color(cx.theme().foreground).text_size(px(self.pref.general.font_size))
            .on_action(cx.listener(|app,_:&NewConnection,w,cx|app.open_dialog(DialogKind::Connection(None),w,cx)))
            .on_action(cx.listener(|app,_:&PreferencesAction,w,cx|app.open_dialog(DialogKind::Preferences,w,cx)))
            .on_action(cx.listener(|app,_:&CancelDialog,_,cx|{app.dialog=None;app.full_editor=false;cx.notify();}))
            .on_action(cx.listener(|app,_:&SaveValue,w,cx|app.save_value(w,cx)))
            .on_action(cx.listener(|app,_:&CloseTab,w,cx|{if let Some(id)=app.active{app.close_tab(id,w,cx);}}))
            .on_action(cx.listener(|app,_:&Refresh,w,cx|{if let Some(id)=app.active{app.scan_keys(id,true,false,w,cx);app.reload_detail(id,true,w,cx);}}))
            .on_action(cx.listener(|app,_:&FocusFilter,w,cx|{if let Some(i)=app.index(){app.tabs[i].filter.update(cx,|input,cx|input.focus(w,cx));}}))
            .child(title).child(div().flex().flex_1().min_h_0().child(ribbon).child(div().flex_1().min_w_0().child(content)));
        if let Some((text,error,_))=&self.toast{root=root.child(div().absolute().top(px(45.)).right(px(16.)).max_w(px(600.)).p_3().rounded(px(5.)).shadow_md().border_1().border_color(if *error{rgb(0xef4444).into()}else{cx.theme().border}).bg(cx.theme().background).child(text.clone()));}
        if self.busy>0 {root=root.child(div().absolute().bottom(px(3.)).right(px(8.)).text_xs().text_color(cx.theme().muted_foreground).child(format!("{}…",self.tr("common.loading"))));}
        if self.dialog.is_some(){root=root.child(self.render_dialog(window,cx));}root.into_any_element()
    }
    fn render_title(&self,cx:&Context<Self>)->AnyElement {
        let caption=if self.nav==Nav::Browser{self.index().map(|i|format!(" - {}",self.tabs[i].conn.name)).unwrap_or_default()}else{String::new()};
        let mut row=div().id("titlebar").flex().items_end().h(px(38.)).flex_shrink_0().bg(self.sidebar_color(cx)).border_b_1().border_color(cx.theme().border)
            .on_mouse_down(MouseButton::Left,|event,w,_|{if event.click_count==2{w.zoom_window();}else{w.start_window_move();}})
            .child(div().flex().items_center().pl(px(if cfg!(target_os="macos"){70.}else{10.})).w(px(self.pref.behavior.aside_width+50.)).h_full().flex_shrink_0()
                .child(img(ImageSource::Resource(Resource::Embedded("images/icon.png".into()))).size(px(25.))).child(div().font_weight(FontWeight::EXTRA_BOLD).child("Tiny RDM")).child(div().text_size(px(13.)).truncate().child(caption)));
        if self.nav==Nav::Browser{for t in &self.tabs{let id=t.id;let selected=self.active==Some(id);row=row.child(div().id(("connection-tab",id)).flex().items_center().gap_2().h(px(33.)).px_3().rounded_t(px(4.)).border_1().border_b_0().border_color(cx.theme().border).bg(if selected{cx.theme().background}else{self.sidebar_color(cx)}).cursor_pointer().text_color(if selected{cx.theme().primary}else{cx.theme().foreground})
            .on_click(cx.listener(move|app,_,_,cx|{app.active=Some(id);cx.notify();})).child(icon("server").size(px(17.))).child(t.conn.name.clone())
            .child(self.icon_btn(format!("close-{id}"),"close","Close",cx,move|app,w,cx|app.close_tab(id,w,cx))));}}
        row.into_any_element()
    }
    fn render_ribbon(&self,cx:&Context<Self>)->AnyElement {
        let mut rail=div().flex().flex_col().w(px(50.)).flex_shrink_0().h_full().bg(self.ribbon_color(cx)).border_r_1().border_color(cx.theme().border);
        for (nav,ico,label) in [(Nav::Browser,"database","ribbon.browser"),(Nav::Server,"server","ribbon.server"),(Nav::Log,"record","ribbon.log")]{if nav==Nav::Browser&&self.tabs.is_empty(){continue;}let selected=self.nav==nav;
            rail=rail.child(div().h(px(40.)).flex().items_center().relative().child(self.icon_btn(format!("nav-{ico}"),ico,self.tr(label),cx,move|app,_,cx|{app.nav=nav;cx.notify();}).size(px(40.)).text_color(if selected{cx.theme().primary}else{cx.theme().muted_foreground})).when(selected,|d|d.child(div().absolute().left_0().h(px(20.)).w(px(3.)).rounded(px(3.)).bg(cx.theme().primary))));}
        let weak=cx.weak_entity();let preferences=self.tr("menu.preferences");let about=self.tr("menu.about");
        let menu=self.icon_btn("settings","config",self.tr("menu.preferences"),cx,|_,_,_|{}).size(px(40.)).dropdown_menu(move|menu,_,_|menu
            .item(menu_action(preferences.clone(),weak.clone(),|app,w,cx|app.open_dialog(DialogKind::Preferences,w,cx)))
            .item(PopupMenuItem::link("用户指南","https://tinyrdm.com/zh/guide/"))
            .item(PopupMenuItem::link("报告问题","https://github.com/tiny-craft/tiny-rdm/issues"))
            .item(menu_action("检查更新",weak.clone(),|app,w,cx|app.check_updates(w,cx)))
            .separator().item(menu_action(about.clone(),weak.clone(),|app,w,cx|app.open_dialog(DialogKind::About,w,cx))));
        rail.child(div().flex_1()).child(menu)
            .child(self.icon_btn("wechat","qrcode",self.tr("ribbon.wechat_official"),cx,|app,w,cx|app.open_dialog(DialogKind::Community,w,cx)).size(px(40.)))
            .child(self.icon_btn("github","github","Github",cx,|_,_,cx|cx.open_url("https://github.com/tiny-craft/tiny-rdm")).size(px(40.))).into_any_element()
    }
    fn render_empty(&self,cx:&Context<Self>)->AnyElement {div().flex().flex_col().items_center().justify_center().size_full().gap_3().child(icon("database").size(px(48.)).text_color(cx.theme().muted_foreground)).child(div().text_color(cx.theme().muted_foreground).child(self.tr("interface.empty_server_content"))).child(self.btn("empty-new",self.tr("interface.new_conn"),"addlink",cx,|app,w,cx|app.open_dialog(DialogKind::Connection(None),w,cx))).into_any_element()}
    fn render_connections(&self,cx:&Context<Self>)->AnyElement {
        let weak=cx.weak_entity();let mut sidebar=div().flex().flex_col().h_full().bg(self.sidebar_color(cx)).border_r_1().border_color(cx.theme().border)
            .child(div().flex().gap_1().p_1().border_b_1().border_color(cx.theme().border).child(Input::new(&self.conn_filter).cleanable(true).small())
                .child(self.icon_btn("new-conn","addlink",self.tr("interface.new_conn"),cx,|app,w,cx|app.open_dialog(DialogKind::Connection(None),w,cx)))
                .child(self.icon_btn("new-group","addgroup",self.tr("interface.new_group"),cx,|app,w,cx|app.open_dialog(DialogKind::Group(None),w,cx)))
                .child(self.icon_btn("connections-more","more",self.tr("interface.more_action"),cx,|_,_,_|{}).dropdown_menu(move|m,_,_|m
                    .item(menu_action("导入连接…",weak.clone(),|a,w,cx|a.connection_file(false,w,cx)))
                    .item(menu_action("导出连接…",weak.clone(),|a,w,cx|a.connection_file(true,w,cx)))
                    .item(menu_action("断开所有连接",weak.clone(),|a,w,cx|{let ids=a.tabs.iter().map(|t|t.id).collect::<Vec<_>>();for id in ids{a.close_tab(id,w,cx);}}))))) ;
        let filter=text_input(&self.conn_filter,cx).to_lowercase();let mut list=div().id("connection-list").flex_1().min_h_0().overflow_y_scroll().p_1();
        for (group_index,c) in self.connections.iter().enumerate(){if c.category=="group"{
            let name=c.name.clone();let weak=cx.weak_entity();let grp=name.clone();
            let group=div().id(("group",group_index)).flex().items_center().gap_2().h(px(30.)).px_2().child(icon("folder").size(px(19.))).child(format!("{} ({})",c.name,c.connections.len()))
                .context_menu(move|m,_,_|{let name=grp.clone();let n=name.clone();let w=weak.clone();m.item(menu_action("重命名分组",w.clone(),move|a,win,cx|a.open_dialog(DialogKind::Group(Some(name.clone())),win,cx)))
                    .item(menu_action("删除分组",w,move|a,_,cx|{if let Some(i)=a.connections.iter().position(|g|g.category=="group"&&g.name==n){let group=a.connections.remove(i);a.connections.extend(group.connections);a.save_connections();cx.notify();}}))});
            list=list.child(group);for child in &c.connections{if filter.is_empty()||child.name.to_lowercase().contains(&filter){list=list.child(self.connection_row(child,20.,cx));}}
        }else if filter.is_empty()||c.name.to_lowercase().contains(&filter){list=list.child(self.connection_row(c,0.,cx));}}
        if flatten(&self.connections).is_empty(){list=list.child(div().p_5().text_color(cx.theme().muted_foreground).child(self.tr("interface.empty_server_list")));}
        sidebar=sidebar.child(list);sidebar.into_any_element()
    }
    fn connection_row(&self,c:&Connection,indent:f32,cx:&Context<Self>)->AnyElement {
        let config=c.clone();let config2=c.clone();let weak=cx.weak_entity();let connected=self.tabs.iter().any(|t|t.conn.name==c.name);
        div().id(SharedString::from(format!("conn-{}",c.name))).flex().items_center().gap_2().h(px(32.)).pl(px(10.+indent)).rounded(px(3.)).cursor_pointer().hover(|d|d.bg(cx.theme().list_hover))
            .on_click(cx.listener(move|a,_,w,cx|a.open_connection(config.clone(),w,cx))).child(icon(if c.cluster.enable{"cluster"}else{"server"}).size(px(18.)).text_color(if connected{cx.theme().primary}else{cx.theme().muted_foreground})).child(c.name.clone())
            .context_menu(move|m,_,_|{let c=config2.clone();let edit=c.clone();let duplicate=c.clone();let delete=c.clone();let disconnect=c.name.clone();let w=weak.clone();
                m.item(menu_action("打开连接",w.clone(),move|a,win,cx|a.open_connection(c.clone(),win,cx)))
                    .item(menu_action("编辑连接配置",w.clone(),move|a,win,cx|a.open_dialog(DialogKind::Connection(Some(edit.clone())),win,cx)))
                    .item(menu_action("复制连接",w.clone(),move|a,win,cx|{let mut d=duplicate.clone();d.name=format!("{} Copy",d.name);a.open_dialog(DialogKind::ConnectionCopy(d),win,cx);}))
                    .item(menu_action("断开连接",w.clone(),move|a,win,cx|{if let Some(t)=a.tabs.iter().find(|t|t.conn.name==disconnect){a.close_tab(t.id,win,cx);}}))
                    .separator().item(menu_action("删除连接",w,move|a,win,cx|a.open_dialog(DialogKind::DeleteConnection(delete.name.clone()),win,cx)))
            }).into_any_element()
    }
    fn render_browser(&self,cx:&Context<Self>)->AnyElement {
        let Some(i)=self.index()else{return div().into_any_element();};let t=&self.tabs[i];let id=t.id;let weak=cx.weak_entity();
        let mut pane=div().flex().flex_col().h_full().bg(self.sidebar_color(cx)).border_r_1().border_color(cx.theme().border)
            .child(div().flex().items_center().gap_1().p_1().border_b_1().border_color(cx.theme().border)
                .child(self.btn("type-filter",if t.key_type.is_empty(){"A"}else{&t.key_type},"",cx,|_,_,_|{}).compact().dropdown_menu(move|mut m,_,_|{for kind in ["","STRING","HASH","LIST","SET","ZSET","STREAM","JSON"]{let w=weak.clone();let kind=kind.to_string();m=m.item(menu_action(if kind.is_empty(){"ALL".into()}else{kind.clone()},w,move|a,w,cx|{if let Some(i)=a.tab_index(id){a.tabs[i].key_type=kind.clone();a.scan_keys(id,true,false,w,cx);}}));}m}))
                .child(Input::new(&t.filter).small().cleanable(true))
                .child(self.icon_btn("filter-keys","search",self.tr("interface.filter_key"),cx,move|a,w,cx|a.scan_keys(id,true,false,w,cx)))
                .child(self.icon_btn("refresh-keys","refresh",self.tr("interface.reload"),cx,move|a,w,cx|a.scan_keys(id,true,false,w,cx)))
                .child(self.icon_btn("new-key","add",self.tr("interface.new_key"),cx,|a,w,cx|a.open_dialog(DialogKind::NewKey,w,cx))));
        let rows=Arc::new(tree_rows(t));let rows2=rows.clone();
        let list=uniform_list("key-tree",rows.len(),cx.processor(move|a,range,_,cx|range.map(|index|a.key_row(id,index,&rows2[index],cx)).collect::<Vec<_>>())).flex_1().min_h_0();pane=pane.child(list);
        let weak=cx.weak_entity();let databases=t.databases.clone();let count=t.keys.len();let total=t.total;let db=t.db;
        let selector=self.btn("database-select",format!("db{db} ({count}/{total})"),"",cx,|_,_,_|{}).flex_1().dropdown_menu(move|mut m,_,_|{for d in &databases{let index=d.index;let label=if d.alias.is_empty(){format!("db{} ({})",d.index,d.keys)}else{format!("db{} - {} ({})",d.index,d.alias,d.keys)};m=m.item(menu_action(label,weak.clone(),move|a,w,cx|a.change_database(id,index,w,cx)));}m});
        let weak=cx.weak_entity();
        let more=self.icon_btn("database-more","more",self.tr("interface.more_action"),cx,|_,_,_|{}).dropdown_menu(move|m,_,_|m
            .item(menu_action("树形 / 平铺列表",weak.clone(),move|a,_,cx|{if let Some(i)=a.tab_index(id){a.tabs[i].flat=!a.tabs[i].flat;cx.notify();}}))
            .item(menu_action("精确匹配",weak.clone(),move|a,w,cx|{if let Some(i)=a.tab_index(id){a.tabs[i].exact=!a.tabs[i].exact;a.scan_keys(id,true,false,w,cx);}}))
            .item(menu_action("导入数据",weak.clone(),|a,w,cx|a.open_dialog(DialogKind::Import,w,cx)))
            .item(menu_action("导出数据",weak.clone(),|a,w,cx|a.open_dialog(DialogKind::Export,w,cx)))
            .item(menu_action("批量删除键",weak.clone(),|a,w,cx|a.open_dialog(DialogKind::DeleteKeys,w,cx)))
            .item(menu_action("为所选项更新TTL",weak.clone(),|a,w,cx|a.open_dialog(DialogKind::Ttl,w,cx)))
            .separator().item(menu_action("清空数据库",weak.clone(),|a,w,cx|a.open_dialog(DialogKind::Flush,w,cx))));
        pane.child(div().flex().gap_1().p_1().border_t_1().border_color(cx.theme().border).child(selector)
            .child(self.icon_btn("load-more-keys","loadlist",self.tr("interface.load_more"),cx,move|a,w,cx|a.scan_keys(id,false,false,w,cx)).disabled(t.scan.end))
            .child(self.icon_btn("load-all-keys","bottom",self.tr("interface.load_all"),cx,move|a,w,cx|a.scan_keys(id,false,true,w,cx)).disabled(t.scan.end))
            .child(self.icon_btn("check-mode","checkbox",self.tr("interface.check_mode"),cx,move|a,_,cx|{if let Some(i)=a.tab_index(id){a.tabs[i].check_mode=!a.tabs[i].check_mode;cx.notify();}}).selected(t.check_mode)).child(more)).into_any_element()
    }
    fn key_row(&self,id:u64,index:usize,row:&TreeRow,cx:&Context<Self>)->AnyElement {
        let t=&self.tabs[self.tab_index(id).unwrap()];let key=row.key.clone();let path=row.path.clone();let folder=key.is_none();let weak=cx.weak_entity();let menu_key=key.clone();let prefix=path.clone();
        let selected=key.as_ref().is_some_and(|k|t.current_key.as_ref()==Some(k));let mut element=div().id(("key-row",index)).flex().items_center().gap_2().h(px(29.)).pl(px(10.+row.depth as f32*16.)).pr_1().cursor_pointer().rounded(px(3.)).when(selected,|d|d.bg(cx.theme().list_active)).hover(|d|d.bg(cx.theme().list_hover));
        if folder{element=element.child(icon(if t.collapsed.contains(&row.path){"right"}else{"down"}).size(px(10.))).child(icon("folder").size(px(19.)));}
        else{if t.check_mode{let k=key.clone().unwrap();element=element.child(Checkbox::new(("key-check",index)).checked(t.selected.contains(&k)).on_click(cx.listener(move|a,_,_,cx|{if let Some(i)=a.tab_index(id){if !a.tabs[i].selected.insert(k.clone()){a.tabs[i].selected.remove(&k);}cx.notify();}})));}element=element.child(type_tag(&row.kind,true));}
        element.child(div().flex_1().truncate().child(row.label.clone())).on_click(cx.listener(move|a,_,w,cx|{if let Some(k)=&key{a.select_key(id,k.clone(),w,cx);}else if let Some(i)=a.tab_index(id){if !a.tabs[i].collapsed.insert(path.clone()){a.tabs[i].collapsed.remove(&path);}cx.notify();}}))
            .context_menu(move|m,_,_|{let k=menu_key.clone();let p=prefix.clone();let w=weak.clone();let kcopy=k.clone();let krename=k.clone();
                m.item(menu_action("复制键名",w.clone(),move|_,_,cx|cx.write_to_clipboard(ClipboardItem::new_string(kcopy.as_ref().map(|b|display_bytes(b)).unwrap_or_else(||p.clone())))))
                    .item(menu_action("重命名键",w.clone(),move|a,win,cx|{if let Some(key)=krename.clone(){a.select_key(id,key,win,cx);a.open_dialog(DialogKind::Rename,win,cx);}}))
                    .item(menu_action("删除键",w,move|a,win,cx|{if let Some(i)=a.tab_index(id){a.tabs[i].selected.clear();if let Some(k)=&k{a.tabs[i].selected.insert(k.clone());}a.open_dialog(DialogKind::DeleteKeys,win,cx);}}))
            }).into_any_element()
    }
    fn render_content(&self,window:&mut Window,cx:&mut Context<Self>)->AnyElement {
        let Some(i)=self.index()else{return self.render_empty(cx);};let t=&self.tabs[i];let id=t.id;
        let mut tabs=div().flex().items_center().h(px(37.)).flex_shrink_0().border_b_1().border_color(cx.theme().border).gap_1().px_1();
        for (pane,ico,key) in [(Pane::Status,"status","status"),(Pane::Detail,"detail","key_detail"),(Pane::Cli,"terminal","cli"),(Pane::Slow,"log","slow_log"),(Pane::Monitor,"monitor","cmd_monitor"),(Pane::Pubsub,"broadcast","pub_message")]{tabs=tabs.child(div().border_b(px(2.)).border_color(if t.pane==pane{cx.theme().primary}else{hsla(0.,0.,0.,0.)}).child(self.btn(format!("pane-{key}"),self.tr(&format!("interface.sub_tab.{key}")),ico,cx,move|a,w,cx|a.switch_pane(id,pane,w,cx)).ghost().px_2().text_color(if t.pane==pane{cx.theme().primary}else{cx.theme().foreground})));}
        let page=match t.pane{Pane::Status=>self.render_status(i,cx),Pane::Detail=>self.render_detail(i,window,cx),Pane::Cli=>self.render_cli(i,cx),Pane::Slow=>self.render_slow(i,cx),Pane::Monitor=>self.render_stream(i,false,cx),Pane::Pubsub=>self.render_stream(i,true,cx)};
        div().flex().flex_col().size_full().child(tabs).child(div().flex_1().min_h_0().child(page)).into_any_element()
    }
    fn render_status(&self,i:usize,cx:&Context<Self>)->AnyElement {
        let t=&self.tabs[i];let id=t.id;let get=|section,key|info_value(&t.info,section,key);let card_color=if self.dark(cx){rgb(0x212121)}else{rgb(0xfafafa)};
        let head=div().flex().items_center().gap_2().child(div().text_size(px(20.)).font_weight(FontWeight::SEMIBOLD).child(t.conn.name.clone()))
            .child(status_tag(format!("v{}",get("Server","redis_version")),cx)).child(status_tag(get("Server","redis_mode"),cx)).child(status_tag(get("Replication","role"),cx)).child(div().flex_1())
            .child(self.icon_btn("status-refresh","refresh",self.tr("interface.reload"),cx,move|a,w,cx|a.refresh_info(id,w,cx)));
        let summary=div().flex().gap_5().mt_5().children([
            ("运行时间",format!("{} 天",get("Server","uptime_in_days"))), ("已连客户端",get("Clients","connected_clients")),
            ("键总数",t.total.to_string()),("内存使用",human_bytes(get("Memory","used_memory").parse().unwrap_or(0.))),
        ].into_iter().map(|(label,value)|div().flex_1().flex().flex_col().gap_2().child(div().text_color(cx.theme().muted_foreground).child(label)).child(div().text_size(px(26.)).child(value))));
        let chart=|label:&str,color:u32,field:fn(&Sample)->f64|{let data=t.samples.iter().map(|s|(s.time.clone(),field(s))).collect::<Vec<_>>();div().flex_1().min_w_0().flex().flex_col().gap_4().child(div().text_center().child(div().inline_flex().gap_2().items_center().child(div().w(px(34.)).h(px(12.)).border(px(2.)).border_color(rgb(color)).bg(rgba((color<<8)|0x24))).child(label.to_string())))
                .child(AreaChart::new(data).x(|d:&(String,f64)|SharedString::from(d.0.clone())).y(|d|d.1).stroke(rgb(color)).fill(rgba((color<<8)|0x28)).natural().tick_margin(10)).h(px(235.))};
        let mut charts=div().id("status-scroll").flex_1().min_h_0().overflow_y_scroll().p_4().gap_5().flex().flex_col();
        if self.info_details{for(section,values)in &t.info{charts=charts.child(div().font_weight(FontWeight::BOLD).child(section.clone()));for(k,v)in values{charts=charts.child(div().flex().border_b_1().border_color(cx.theme().border).py_1().child(div().w(px(260.)).child(k.clone())).child(v.clone()));}}}
        else{charts=charts.child(div().flex().gap_5().child(chart("命令执行数/秒",0xff6384,|s|s.ops)).child(chart("已连客户端",0xff9f40,|s|s.clients)))
            .child(div().flex().gap_5().child(chart("内存使用",0x9966ff,|s|s.memory)).child(chart("网络输入 / 输出",0x36a2eb,|s|s.input+s.output)));}
        div().flex().flex_col().size_full().p_1().gap_1().child(div().rounded(px(4.)).border_1().border_color(cx.theme().border).bg(card_color).p_5().child(head).child(summary))
            .child(div().flex().flex_col().flex_1().min_h_0().rounded(px(4.)).border_1().border_color(cx.theme().border).bg(card_color)
                .child(div().flex().gap_3().border_b_1().border_color(cx.theme().border).px_4().py_2().child(self.btn("activity","活动状态","",cx,|a,_,cx|{a.info_details=false;cx.notify();}).ghost().selected(!self.info_details)).child(self.btn("status-info","状态信息","",cx,|a,_,cx|{a.info_details=true;cx.notify();}).ghost().selected(self.info_details))).child(charts)).into_any_element()
    }
    fn render_detail(&self,i:usize,_window:&mut Window,cx:&Context<Self>)->AnyElement {
        let t=&self.tabs[i];let id=t.id;let Some(d)=&t.detail else{return div().flex().items_center().justify_center().size_full().text_color(cx.theme().muted_foreground).child(self.tr("interface.nonexist_tab_content")).into_any_element();};
        let key=display_bytes(&d.key);let copy_key=key.clone();let ttl=if d.ttl<0{self.tr("interface.forever")}else{format!("{}s",d.ttl)};
        let toolbar=div().flex().gap_1().items_center().p_1().child(type_tag(&d.kind,false)).child(div().flex_1().h(px(32.)).px_2().flex().items_center().border_1().border_color(cx.theme().border).rounded(px(4.)).truncate().child(key))
            .child(self.icon_btn("copy-key","copy",self.tr("interface.copy_key"),cx,move|_,_,cx|cx.write_to_clipboard(ClipboardItem::new_string(copy_key.clone()))))
            .child(self.icon_btn("reload-key","refresh",self.tr("interface.reload"),cx,move|a,w,cx|a.reload_detail(id,true,w,cx)))
            .child(self.icon_btn("auto-refresh","timer",self.tr("interface.auto_refresh"),cx,move|a,_,cx|{if let Some(i)=a.tab_index(id){a.tabs[i].auto_refresh=!a.tabs[i].auto_refresh;cx.notify();}}).selected(t.auto_refresh))
            .child(self.btn("ttl",format!("TTL {ttl}"),"",cx,|a,w,cx|a.open_dialog(DialogKind::Ttl,w,cx)))
            .child(self.icon_btn("rename-key","edit",self.tr("interface.rename_key"),cx,|a,w,cx|a.open_dialog(DialogKind::Rename,w,cx)))
            .child(self.icon_btn("delete-key","delete",self.tr("interface.delete_key"),cx,|a,w,cx|a.open_dialog(DialogKind::DeleteKeys,w,cx)));
        let body=if matches!(d.kind,KeyType::String|KeyType::Json){
            div().flex().flex_col().size_full().child(div().flex().justify_end().gap_1().px_1().py_1()
                .child(self.icon_btn("copy-value","copy",self.tr("interface.copy_value"),cx,move|a,_,cx|{if let Some(i)=a.tab_index(id){cx.write_to_clipboard(ClipboardItem::new_string(text_input(&a.tabs[i].editor,cx)));}}))
                .child(self.btn("save-value",self.tr("interface.save_update"),"save",cx,|a,w,cx|a.save_value(w,cx)).primary()))
                .child(div().flex_1().min_h_0().text_size(px(self.pref.editor.font_size)).child(Input::new(&t.editor).h_full().appearance(false))).into_any_element()
        }else{self.render_entries(i,cx)};
        let weak=cx.weak_entity();let formats=codec::FORMATS.iter().map(|s|s.to_string()).collect::<Vec<_>>();let format_button=self.btn("format",format!("{}: {}",self.tr("interface.view_as"),t.format),"down",cx,|_,_,_|{}).ghost().dropdown_menu(move|mut m,_,_|{for f in &formats{let value=f.clone();m=m.item(menu_action(f.clone(),weak.clone(),move|a,w,cx|a.change_format(id,None,Some(value.clone()),w,cx)));}m});
        let weak=cx.weak_entity();let mut decoders=codec::DECODERS.iter().map(|s|s.to_string()).collect::<Vec<_>>();decoders.extend(self.pref.decoder.iter().filter(|d|d.enable).map(|d|d.name.clone()));let decoder_button=self.btn("decode",format!("{}: {}",self.tr("interface.decode_with"),t.decode),"down",cx,|_,_,_|{}).ghost().dropdown_menu(move|mut m,_,_|{for f in &decoders{let value=f.clone();m=m.item(menu_action(f.clone(),weak.clone(),move|a,w,cx|a.change_format(id,Some(value.clone()),None,w,cx)));}m});
        div().flex().flex_col().size_full().child(toolbar).child(div().flex_1().min_h_0().child(body))
            .child(div().flex().items_center().gap_2().h(px(30.)).px_2().border_t_1().border_color(cx.theme().border).child(format!("{}: {}  |  {}: {}",self.tr("interface.length"),d.length,self.tr("interface.memory_usage"),human_bytes(d.memory as f64))).child(div().flex_1()).child(format_button).child(decoder_button)).into_any_element()
    }
    fn render_entries(&self,i:usize,cx:&Context<Self>)->AnyElement {
        let t=&self.tabs[i];let id=t.id;let d=t.detail.as_ref().unwrap();let rows=Arc::new(d.rows.clone());let kind=d.kind.clone();let weak=cx.weak_entity();
        let header=div().flex().items_center().gap_2().p_1().child(Input::new(&t.row_filter).small())
            .child(self.icon_btn("entry-search","search",self.tr("interface.search"),cx,move|a,w,cx|a.reload_detail(id,true,w,cx)))
            .child(self.btn("add-entry",self.tr("interface.add_row"),"add",cx,|a,w,cx|a.open_dialog(DialogKind::AddEntries,w,cx)));
        let list=uniform_list("entry-table",rows.len(),cx.processor(move|a,range,_,cx|range.map(|index|{
            let row=rows[index].clone();let edit=row.clone();let delete=row.clone();let key=display_bytes(&row.id);let value=codec::convert(&row.value,&a.tabs[a.tab_index(id).unwrap()].decode,&a.tabs[a.tab_index(id).unwrap()].format,&a.pref.decoder).map(|v|v.text).unwrap_or_else(|_|display_bytes(&row.value));let k=kind.clone();let w=weak.clone();
            div().id(("entry",index)).flex().items_center().h(px(34.)).border_b_1().border_color(cx.theme().border).px_2().gap_2().when(index%2==1,|d|d.bg(cx.theme().muted))
                .child(div().w(px(50.)).child((index+1).to_string())).child(div().w(px(180.)).truncate().child(key)).when(k==KeyType::ZSet,|d|d.child(div().w(px(70.)).child(row.score.clone())))
                .child(div().flex_1().truncate().child(value)).child(a.icon_btn(format!("entry-edit-{index}"),"edit",a.tr("interface.edit_row"),cx,move|a,w,cx|a.open_dialog(DialogKind::EditEntry(edit.clone()),w,cx)))
                .child(a.icon_btn(format!("entry-delete-{index}"),"delete",a.tr("interface.delete_row"),cx,move|a,w,cx|a.remove_entry(id,delete.clone(),w,cx)))
                .context_menu(move|m,_,_|{let r=row.clone();m.item(menu_action("复制值",w.clone(),move|_,_,cx|cx.write_to_clipboard(ClipboardItem::new_string(display_bytes(&r.value)))))})
                .into_any_element()
        }).collect::<Vec<_>>())).flex_1().min_h_0();
        div().flex().flex_col().size_full().child(header).child(div().flex().h(px(28.)).items_center().px_2().gap_2().bg(cx.theme().muted).font_weight(FontWeight::MEDIUM).child(div().w(px(50.)).child("#")).child(div().w(px(180.)).child("Field / Index / Member / ID")).child("Value"))
            .child(list).child(div().flex().justify_end().gap_2().p_1().child(self.btn("more-entries",self.tr("interface.load_more_entries"),"loadlist",cx,move|a,w,cx|a.reload_detail(id,false,w,cx)).disabled(d.end))).into_any_element()
    }
    fn render_cli(&self,i:usize,cx:&Context<Self>)->AnyElement {
        let t=&self.tabs[i];let id=t.id;let output=t.cli_output.clone();
        div().flex().flex_col().size_full().bg(rgb(if self.dark(cx){0x1e1e1e}else{0xffffff})).font_family("Menlo").text_size(px(self.pref.cli.font_size))
            .child(div().id("cli-output").flex_1().min_h_0().overflow_y_scroll().p_3().child(output))
            .child(div().flex().items_center().px_2().pb_2().gap_2().child(format!("{}:{}[{}]>",t.conn.addr,t.conn.port,t.cli_db)).child(Input::new(&t.cli_input).appearance(false))
                .child(self.icon_btn("cli-execute","play","Execute",cx,move|a,w,cx|a.execute_cli(id,w,cx)))).into_any_element()
    }
    fn render_slow(&self,i:usize,cx:&Context<Self>)->AnyElement {
        let t=&self.tabs[i];let id=t.id;let mut list=div().id("slow-list").overflow_y_scroll().flex_1().min_h_0();
        for log in &t.slow {let time=chrono::DateTime::from_timestamp(log.time,0).map(|d|d.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S").to_string()).unwrap_or_default();list=list.child(div().flex().border_b_1().border_color(cx.theme().border).p_2().gap_2().child(div().w(px(50.)).child(log.id.to_string())).child(div().w(px(160.)).child(time)).child(div().w(px(100.)).child(format!("{} μs",log.duration))).child(div().flex_1().child(log.args.clone())));}
        div().flex().flex_col().size_full().child(div().flex().justify_end().p_1().child(self.icon_btn("slow-refresh","refresh",self.tr("interface.reload"),cx,move|a,w,cx|a.load_slow(id,w,cx))))
            .child(div().flex().p_2().gap_2().bg(cx.theme().muted).child(div().w(px(50.)).child("ID")).child(div().w(px(160.)).child("Time")).child(div().w(px(100.)).child("Duration")).child("Command")).child(list).into_any_element()
    }
    fn render_stream(&self,i:usize,pubsub:bool,cx:&Context<Self>)->AnyElement {
        let t=&self.tabs[i];let id=t.id;let stream=if pubsub{&t.pubsub}else{&t.monitor};let filter=text_input(&t.stream_filter,cx);let lines=stream.as_ref().map(|s|s.lines.iter().filter(|s|filter.is_empty()||s.contains(&filter)).cloned().collect::<Vec<_>>().join("\n")).unwrap_or_default();
        let head=div().flex().items_center().gap_2().p_2().child(Input::new(&t.stream_filter).small()).when(pubsub,|d|d.child(Input::new(&t.pub_pattern).small()))
            .child(self.btn("stream-start",if stream.is_some(){"停止"}else{"开始"},if stream.is_some(){"pause"}else{"play"},cx,move|a,w,cx|a.stream_toggle(id,pubsub,w,cx)))
            .child(self.icon_btn("stream-clear","clear","Clear",cx,move|a,_,cx|{if let Some(i)=a.tab_index(id){let s=if pubsub{&mut a.tabs[i].pubsub}else{&mut a.tabs[i].monitor};if let Some(s)=s{s.lines.clear();cx.notify();}}}))
            .child(self.icon_btn("stream-export","export","Export",cx,move|a,w,cx|a.export_stream(id,pubsub,w,cx)));
        let mut content=div().flex().flex_col().size_full().child(head).child(div().id("stream-output").flex_1().min_h_0().overflow_y_scroll().p_2().font_family("Menlo").text_size(px(12.)).child(lines));
        if pubsub{content=content.child(div().flex().flex_col().p_2().gap_2().border_t_1().border_color(cx.theme().border).child(Input::new(&t.pub_channel)).child(Input::new(&t.pub_message).h(px(80.))).child(self.btn("publish","发布","publish",cx,move|a,w,cx|a.publish(id,w,cx)).primary()));}content.into_any_element()
    }
    fn render_log(&self,cx:&Context<Self>)->AnyElement {
        let filter=text_input(&self.log_filter,cx).to_lowercase();let logs=self.service.try_lock().map(|s|s.history.iter().rev().filter(|l|filter.is_empty()||l.command.to_lowercase().contains(&filter)||l.server.to_lowercase().contains(&filter)).skip(self.log_page*100).take(100).cloned().collect::<Vec<_>>()).unwrap_or_default();
        let mut list=div().id("operation-log-list").flex_1().min_h_0().overflow_y_scroll();for log in logs{list=list.child(div().flex().gap_2().p_2().border_b_1().border_color(cx.theme().border).child(div().w(px(180.)).child(log.time)).child(div().w(px(120.)).child(format!("{} / db{}",log.server,log.db))).child(div().w(px(90.)).child(format!("{:.2} ms",log.elapsed_ms))).child(div().flex_1().child(log.command)).when(!log.error.is_empty(),|d|d.text_color(cx.theme().danger).child(log.error)));}
        div().flex().flex_col().size_full().child(div().flex().gap_2().p_2().child(Input::new(&self.log_filter).cleanable(true)).child(self.btn("clear-logs","清空","clear",cx,|a,w,cx|a.job(w,cx,|s|{s.history.clear();Ok(())},|_,_,_,_|{}))))
            .child(list).child(div().flex().justify_end().p_2().gap_2().child(self.btn("log-prev","‹","",cx,|a,_,cx|{a.log_page=a.log_page.saturating_sub(1);cx.notify();})).child((self.log_page+1).to_string()).child(self.btn("log-next","›","",cx,|a,_,cx|{a.log_page+=1;cx.notify();}))).into_any_element()
    }
    fn change_format(&mut self,id:u64,decode:Option<String>,format:Option<String>,window:&mut Window,cx:&mut Context<Self>) {if let Some(i)=self.tab_index(id){let t=&mut self.tabs[i];if let Some(v)=decode{t.decode=v;}if let Some(v)=format{t.format=v;}if let Some(d)=&t.detail&&matches!(d.kind,KeyType::String|KeyType::Json){match codec::convert(&d.value,&t.decode,&t.format,&self.pref.decoder){Ok(v)=>set_input(&t.editor,v.text,window,cx),Err(e)=>self.toast(e.to_string(),true)}}cx.notify();}}
    fn remove_entry(&mut self,id:u64,row:Entry,window:&mut Window,cx:&mut Context<Self>){let Some(i)=self.tab_index(id)else{return;};let t=&self.tabs[i];let Some(d)=&t.detail else{return;};let c=t.conn.clone();let db=t.db;let key=d.key.clone();let kind=d.kind.clone();self.job(window,cx,move|s|s.edit_entry(&c,db,&key,&kind,&row,&row,true),move|a,_,w,cx|a.reload_detail(id,true,w,cx));}
}
fn status_tag(text:String,cx:&App)->AnyElement {div().px_2().py_1().text_size(px(12.)).rounded(px(3.)).border_1().border_color(cx.theme().primary).text_color(cx.theme().primary).child(text).into_any_element()}
fn type_tag(kind:&KeyType,short:bool)->AnyElement {let(c,b)=kind.colors();div().flex().items_center().justify_center().h(px(if short{22.}else{32.})).min_w(px(if short{22.}else{65.})).px(px(if short{0.}else{8.})).rounded(px(4.)).border_1().border_color(rgba(0xddddddcc)).bg(rgb(b)).text_color(rgb(c)).text_size(px(12.)).font_weight(FontWeight::BOLD).child(if short{kind.short().to_string()}else{kind.name().to_string()}).into_any_element()}
#[derive(Clone)] struct TreeRow {key:Option<Vec<u8>>,label:String,path:String,depth:usize,kind:KeyType}
#[derive(Default)] struct Node {children:BTreeMap<String,Node>,keys:Vec<KeyEntry>,count:usize}
fn tree_rows(tab:&Tab)->Vec<TreeRow>{
    if tab.flat{return tab.keys.iter().map(|k|TreeRow{key:Some(k.key.clone()),label:display_bytes(&k.key),path:String::new(),depth:0,kind:k.kind.clone()}).collect();}
    let sep=&tab.conn.key_separator;let mut root=Node::default();for k in &tab.keys{let label=display_bytes(&k.key);let parts=label.split(sep).collect::<Vec<_>>();let mut n=&mut root;n.count+=1;for p in &parts[..parts.len().saturating_sub(1)]{n=n.children.entry(p.to_string()).or_default();n.count+=1;}n.keys.push(k.clone());}
    fn visit(n:&Node,path:String,depth:usize,tab:&Tab,out:&mut Vec<TreeRow>){for(label,node)in &n.children{let path=format!("{path}{label}{}",tab.conn.key_separator);out.push(TreeRow{key:None,label:format!("{label} ({})",node.count),path:path.clone(),depth,kind:KeyType::None});if !tab.collapsed.contains(&path){visit(node,path,depth+1,tab,out);}}for k in &n.keys{out.push(TreeRow{key:Some(k.key.clone()),label:display_bytes(&k.key).rsplit(&tab.conn.key_separator).next().unwrap_or_default().into(),path:path.clone(),depth,kind:k.kind.clone()});}}
    let mut rows=vec![];visit(&root,String::new(),0,tab,&mut rows);rows
}
