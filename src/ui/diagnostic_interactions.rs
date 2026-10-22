//! Actual UI actions and disposable Redis/configuration outcomes.
use super::*;

async fn settle(window: WindowHandle<Root>, cx: &mut AsyncApp) {
    cx.update_window(window.into(), |_, w, cx| { w.refresh(); let _=w.draw(cx); }).unwrap();
    cx.background_executor().timer(Duration::from_millis(30)).await;
}

async fn until(view: &Entity<RdmApp>, window: WindowHandle<Root>, cx: &mut AsyncApp,
    description: &str, condition: impl Fn(&RdmApp, &App) -> bool) {
    for _ in 0..150 {
        if window.update(cx, |_, _, cx| condition(view.read(cx), cx)).unwrap() { return; }
        cx.background_executor().timer(Duration::from_millis(20)).await;
    }
    panic!("diagnostic interaction did not complete: {description}");
}

fn run(view: &Entity<RdmApp>, window: WindowHandle<Root>, cx: &mut AsyncApp, db: u32, args: Vec<Vec<u8>>) -> crate::redis::Value {
    window.update(cx, |_, _, cx| {
        let app=view.read(cx);let tab=&app.tabs[app.index().unwrap()];
        assert_eq!(tab.conn.network,"unix");
        assert_eq!(std::fs::read(app.service.store.root.join("RDM-QA-FIXTURE")).unwrap(),b"rust-rdm-render-fixture-v1\n");
        app.service.run(&tab.conn,db,args).unwrap()
    }).unwrap()
}

async fn copy(view: &Entity<RdmApp>, window: WindowHandle<Root>, cx: &mut AsyncApp, id: &str, expected: String) {
    let original=cx.update(|cx|cx.read_from_clipboard());
    qa_json::click(id,window,cx);
    let actual=cx.update(|cx|qa::copy_result(original,cx));
    assert_eq!(actual,Some(expected),"actual diagnostic copy; clipboard restored before assertion");
    let _=view;
}

pub(super) async fn verify(view: &Entity<RdmApp>, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    window.update(cx,|_,w,cx|view.update(cx,|app,cx|{
        if app.active.is_none(){app.open_connection(app.connections[0].clone(),w,cx);}
        app.nav=Nav::Browser;app.pref.general.theme="light".into();app.apply_theme(w,cx);
    })).unwrap();
    qa_json::idle(view,window,cx).await;
    window.update(cx,|_,_,cx|view.update(cx,|app,cx|{
        let i=app.index().unwrap();app.tabs[i].pane=Pane::Monitor;app.tabs[i].monitor=Some(StreamState::idle());cx.notify();
    })).unwrap();
    settle(window,cx).await;
    qa_json::click("stream-last",window,cx);
    assert!(!window.update(cx,|_,_,cx|view.read(cx).tabs[view.read(cx).index().unwrap()].monitor.as_ref().unwrap().auto_last).unwrap());
    qa_json::click("stream-start",window,cx);
    cx.background_executor().timer(Duration::from_millis(120)).await;
    run(view,window,cx,0,command(&["PING","__rdm_monitor_first__"]));
    until(view,window,cx,"first actual MONITOR frame",|app,_|app.tabs[app.index().unwrap()].monitor.as_ref().unwrap().lines.iter().any(|line|line.text.contains("__rdm_monitor_first__"))).await;
    qa_json::click("stream-start",window,cx);
    cx.background_executor().timer(Duration::from_millis(1100)).await;
    qa_json::click("stream-start",window,cx);
    assert!(!window.update(cx,|_,_,cx|view.read(cx).tabs[view.read(cx).index().unwrap()].monitor.as_ref().unwrap().auto_last).unwrap());
    cx.background_executor().timer(Duration::from_millis(120)).await;
    run(view,window,cx,0,command(&["PING","__rdm_monitor_second__"]));
    until(view,window,cx,"restarted actual MONITOR frame",|app,_|app.tabs[app.index().unwrap()].monitor.as_ref().unwrap().lines.iter().any(|line|line.text.contains("__rdm_monitor_second__"))).await;
    qa_json::click("stream-start",window,cx);
    cx.background_executor().timer(Duration::from_millis(1100)).await;
    let expected=window.update(cx,|_,_,cx|view.read(cx).tabs[view.read(cx).index().unwrap()].monitor.as_ref().unwrap().lines.iter().map(StreamLine::export).collect::<Vec<_>>().join("\n")).unwrap();
    copy(view,window,cx,"stream-copy",expected).await;
    qa_json::click("stream-clear",window,cx);
    assert!(window.update(cx,|_,_,cx|view.read(cx).tabs[view.read(cx).index().unwrap()].monitor.as_ref().unwrap().lines.is_empty()).unwrap());

    window.update(cx,|_,_,cx|view.update(cx,|app,cx|{let i=app.index().unwrap();app.tabs[i].pane=Pane::Pubsub;app.tabs[i].pubsub=Some(StreamState::idle());cx.notify();})).unwrap();
    settle(window,cx).await;
    qa_json::click("stream-last",window,cx);
    qa_json::click("stream-start",window,cx);
    let mut ready=false;
    for _ in 0..100 {
        if run(view,window,cx,0,command(&["PUBSUB","NUMPAT"])).integer().unwrap()>0 {ready=true;break;}
        cx.background_executor().timer(Duration::from_millis(20)).await;
    }
    assert!(ready,"actual PSUBSCRIBE acknowledgement");
    window.update(cx,|_,w,cx|view.update(cx,|app,cx|{let i=app.index().unwrap();set_input(&app.tabs[i].pub_channel," ",w,cx);set_input(&app.tabs[i].pub_message,"__rdm_pubsub_actual__",w,cx);cx.notify();})).unwrap();
    settle(window,cx).await;
    qa_json::click("publish",window,cx);
    qa_json::idle(view,window,cx).await;
    until(view,window,cx,"space channel actual PUBLISH/PSUBSCRIBE",|app,cx|{let t=&app.tabs[app.index().unwrap()];t.pub_received.as_ref().is_some_and(|(n,_)|*n==1)&&text_input(&t.pub_message,cx).is_empty()&&t.pubsub.as_ref().unwrap().lines.iter().any(|line|line.channel.as_deref()==Some(" ")&&line.text=="__rdm_pubsub_actual__")}).await;
    copy(view,window,cx,"message-copy-0","__rdm_pubsub_actual__".into()).await;
    qa_json::click("stream-start",window,cx);
    cx.background_executor().timer(Duration::from_millis(1100)).await;
    qa_json::click("stream-clear",window,cx);

    let log_state=keypages_qa::prepare(view,window,"keypages-log-rows-light",cx).await;
    window.update(cx,|_,w,cx|view.update(cx,|app,cx|{set_input(&app.log_filter,"get",w,cx);cx.notify();})).unwrap();
    settle(window,cx).await;
    assert!(window.update(cx,|_,w,_|gpui_base::test_support::find(w,&[],&"log-row-0".into()).is_none()).unwrap(),"original case-sensitive log filtering");
    window.update(cx,|_,w,cx|view.update(cx,|app,cx|{set_input(&app.log_filter,"GET",w,cx);cx.notify();})).unwrap();
    settle(window,cx).await;
    assert!(window.update(cx,|_,w,_|gpui_base::test_support::find(w,&[],&"log-row-0".into()).unwrap().label().unwrap().contains("GET astring")).unwrap());
    window.update(cx,|_,w,cx|view.update(cx,|app,cx|{set_input(&app.log_filter,"",w,cx);cx.notify();})).unwrap();
    for expected in [None,Some(false),Some(true)] {
        qa_json::click("log-sort-time",window,cx);
        assert_eq!(window.update(cx,|_,_,cx|view.read(cx).log_ascending).unwrap(),expected);
    }
    qa_json::click("clear-logs",window,cx);qa_json::click("dialog-cancel",window,cx);
    assert_eq!(window.update(cx,|_,_,cx|view.read(cx).service.history.lock().len()).unwrap(),2);
    qa_json::click("clear-logs",window,cx);qa_json::click("dialog-save",window,cx);
    assert!(window.update(cx,|_,_,cx|view.read(cx).service.history.lock().is_empty()).unwrap());
    window.update(cx,|_,w,cx|view.update(cx,|app,cx|keypages_qa::restore(log_state,app,w,cx))).unwrap();

    let ttl_state=keypages_qa::prepare(view,window,"keypages-ttl-days-light",cx).await;
    qa_json::click("ttl-quick-10-1",window,cx);qa_json::click("dialog-save",window,cx);
    qa_json::idle(view,window,cx).await;
    let ttl=run(view,window,cx,0,command(&["PTTL","aastring"])).integer().unwrap();
    run(view,window,cx,0,command(&["PERSIST","aastring"]));
    assert!((8000..=10000).contains(&ttl),"quick TTL uses seconds instead of the previous day unit: {ttl}");
    window.update(cx,|_,w,cx|view.update(cx,|app,cx|keypages_qa::restore(ttl_state,app,w,cx))).unwrap();

    let rename_state=keypages_qa::prepare(view,window,"keypages-rename-light",cx).await;
    assert_eq!(run(view,window,cx,0,command(&["EXISTS","__rdm_qa_renamed__"])).integer().unwrap(),0);
    window.update(cx,|_,w,cx|view.update(cx,|app,cx|app.set_dialog_field("new_key","__rdm_qa_renamed__",w,cx))).unwrap();
    qa_json::click("dialog-save",window,cx);qa_json::idle(view,window,cx).await;
    let exists=run(view,window,cx,0,command(&["EXISTS","__rdm_qa_renamed__"])).integer().unwrap();
    if exists==1 {run(view,window,cx,0,command(&["RENAME","__rdm_qa_renamed__","aastring"]));}
    assert_eq!(exists,1,"actual rename confirmation");
    window.update(cx,|_,w,cx|view.update(cx,|app,cx|keypages_qa::restore(rename_state,app,w,cx))).unwrap();

    assert_eq!(run(view,window,cx,15,command(&["DBSIZE"])).integer().unwrap(),0);
    for key in ["__rdm_batch_1__","__rdm_batch_2__"] {run(view,window,cx,15,command(&["SET",key,"owned"]));}
    window.update(cx,|_,w,cx|view.update(cx,|app,cx|{let id=app.active.unwrap();app.change_database(id,15,w,cx);})).unwrap();
    qa_json::idle(view,window,cx).await;
    window.update(cx,|_,w,cx|view.update(cx,|app,cx|{app.open_dialog(dialogs::DialogKind::DeleteKeys,w,cx);app.set_dialog_keys(vec![b"__rdm_batch_1__".to_vec(),b"__rdm_batch_2__".to_vec()]);})).unwrap();
    qa_json::click("dialog-save",window,cx);qa_json::idle(view,window,cx).await;
    assert_eq!(run(view,window,cx,15,command(&["DBSIZE"])).integer().unwrap(),0,"actual selected batch deletion");
    run(view,window,cx,15,command(&["SET","__rdm_flush__","owned"]));
    window.update(cx,|_,w,cx|view.update(cx,|app,cx|app.open_dialog(dialogs::DialogKind::Flush,w,cx))).unwrap();
    qa_json::click("dialog-save",window,cx);
    assert_eq!(run(view,window,cx,15,command(&["DBSIZE"])).integer().unwrap(),1,"unchecked destructive confirmation is disabled");
    qa_json::click("check-confirm",window,cx);qa_json::click("dialog-save",window,cx);qa_json::idle(view,window,cx).await;
    assert_eq!(run(view,window,cx,15,command(&["DBSIZE"])).integer().unwrap(),0,"actual confirmed FLUSHDB");
    window.update(cx,|_,w,cx|view.update(cx,|app,cx|{let id=app.active.unwrap();app.change_database(id,0,w,cx);})).unwrap();
    qa_json::idle(view,window,cx).await;
    println!("DIAGNOSTIC FUNCTIONAL QA PASS: actual monitor start/stop/restart/auto-last/copy/clear; actual whitespace-channel publish/subscribe/copy; original case-sensitive log filtering and three-state sort; clear confirmation; quick TTL, rename, selected batch deletion and guarded DB15 flush. Two clipboard values and owned Redis keys restored before assertions.");
}
