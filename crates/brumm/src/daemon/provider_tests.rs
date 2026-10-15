use super::*;
use crate::provider::tests::Fixture;

fn daemon(failure: Option<Error>) -> Arc<Daemon> {
    let d = Arc::new(Daemon::new("dev", Library::default(), None));
    let session = Arc::new(ProviderSession {
        api: Arc::new(Fixture {
            failure,
            ..Default::default()
        }),
        lib: d.library(),
    });
    d.lock().api = Some(session);
    d
}

#[tokio::test]
async fn switching_services_preserves_both_saved_connections() {
    let Some(_env) = config::testenv::isolated() else {
        return;
    };
    let cfg = Config {
        provider: "apple".into(),
        user_token: "saved-apple-account".into(),
        tidal: crate::provider::tidal::auth::Session {
            // Invalid client prevents a fixture from sending live requests.
            client_id: "fixture-client".into(),
            user_id: "saved-tidal-account".into(),
            country: "DE".into(),
            refresh_token: "saved-tidal-refresh".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    cfg.save().unwrap();
    let d = daemon(None);
    for id in ["tidal", "apple", "tidal"] {
        let mut reply = Message::default();
        tokio::time::timeout(Duration::from_secs(2), d.select_provider(id, &mut reply))
            .await
            .unwrap()
            .unwrap();
        let selection = reply.provider.unwrap();
        assert_eq!(selection.selected, id);
        assert!(!selection.needs_setup);
        assert!(selection.choices.iter().all(|p| p.configured));
        let saved = Config::load().unwrap();
        assert_eq!(saved.user_token, cfg.user_token);
        assert_eq!(saved.tidal.refresh_token, cfg.tidal.refresh_token);
    }
}

#[tokio::test]
async fn requests_from_an_old_session_never_reach_the_new_provider() {
    let d = daemon(None);
    let old = d.session.current();
    {
        let _exclusive = d.session.advance().await;
    }
    for cmd in [
        ipc::CMD_SEARCH,
        ipc::CMD_FAVORITE,
        ipc::CMD_PLAYLIST,
        ipc::CMD_PLACE,
    ] {
        let r = Request {
            generation: old,
            cmd: cmd.into(),
            ..Default::default()
        };
        let mut reply = Message::default();
        assert!(
            d.handle(0, &r, &mut reply)
                .await
                .unwrap_err()
                .to_string()
                .contains("session changed")
        );
        assert!(reply.items.is_empty() && reply.shelves.is_empty());
    }
}

#[tokio::test]
async fn ipc_requests_use_the_injected_provider() {
    let d = daemon(None);
    let requests = [
        Request {
            cmd: ipc::CMD_SEARCH.into(),
            query: "query".into(),
            ..Default::default()
        },
        Request {
            cmd: ipc::CMD_LIST.into(),
            list: ipc::LIST_PLAYLISTS.into(),
            ..Default::default()
        },
        Request {
            cmd: ipc::CMD_OPEN.into(),
            item: Some(Item {
                kind: KIND_ALBUM.into(),
                id: "fixture-album".into(),
                ..Default::default()
            }),
            ..Default::default()
        },
        Request {
            cmd: ipc::CMD_RESOLVE.into(),
            query: "https://example.org/music/track".into(),
            ..Default::default()
        },
    ];
    for request in requests {
        let mut reply = Message::default();
        d.handle(0, &request, &mut reply).await.unwrap();
        match request.cmd.as_str() {
            ipc::CMD_SEARCH => assert_eq!(reply.shelves[0].title, "query"),
            ipc::CMD_LIST => assert_eq!(reply.items[0].id, "fixture-playlist"),
            ipc::CMD_OPEN => assert_eq!(reply.tracks[0].id, "fixture-song"),
            ipc::CMD_RESOLVE => {
                assert_eq!(reply.items[0].id, request.query);
                assert_eq!(reply.ids, ["fixture-song"]);
            }
            _ => unreachable!(),
        }
    }
}

#[tokio::test]
async fn incomplete_lists_and_tracks_are_displayed_but_not_cached() {
    let d = daemon(Some(Error::Partial));
    let api = d.client().unwrap();
    let mut reply = Message::default();
    d.fetch_list(&api, ipc::LIST_PLAYLISTS, &mut reply)
        .await
        .unwrap();
    assert_eq!(reply.items.len(), 1);
    assert!(d.library().items(ipc::LIST_PLAYLISTS).is_none());
    let item = Item {
        kind: KIND_ALBUM.into(),
        id: "fixture-album".into(),
        ..Default::default()
    };
    d.open(&item, &mut reply).await.unwrap();
    assert_eq!(reply.tracks.len(), 1);
    assert!(d.library().tracks(&item.key()).is_none());
}

#[tokio::test]
async fn provider_authorization_failure_updates_session_state() {
    let d = daemon(Some(Error::Unauthorized));
    assert_eq!(
        d.handle(
            0,
            &Request {
                cmd: ipc::CMD_SEARCH.into(),
                query: "query".into(),
                ..Default::default()
            },
            &mut Message::default()
        )
        .await,
        Err(Error::Unauthorized)
    );
    assert_eq!(d.lock().state.status, ipc::STATUS_LOGGED_OUT);
}

#[tokio::test]
async fn refresh_keeps_other_lists_when_one_request_fails() {
    let Some(_env) = config::testenv::isolated() else {
        return;
    };
    let d = daemon(Some(err("temporary playlist failure")));
    d.library().set_items(
        ipc::LIST_PLAYLISTS,
        vec![Item {
            id: "cached-playlist".into(),
            ..Default::default()
        }],
    );
    d.refresh_with(&d.client().unwrap()).await;
    assert_eq!(
        d.library().items(ipc::LIST_PLAYLISTS).unwrap()[0].id,
        "cached-playlist"
    );
    for name in [ipc::LIST_ALBUMS, ipc::LIST_ARTISTS] {
        assert_eq!(d.library().items(name), Some(vec![]));
        assert_eq!(Library::load().items(name), Some(vec![]));
    }
    assert_eq!(d.library().tracks(ipc::LIST_SONGS), Some(vec![]));
}

#[tokio::test]
async fn refresh_stops_when_authorization_fails() {
    let Some(_env) = config::testenv::isolated() else {
        return;
    };
    let d = daemon(Some(Error::Unauthorized));
    d.refresh_with(&d.client().unwrap()).await;
    assert_eq!(d.lock().state.status, ipc::STATUS_LOGGED_OUT);
    assert!(d.library().items(ipc::LIST_ALBUMS).is_none());
}

#[tokio::test]
async fn reloading_the_same_account_keeps_unsaved_library_and_queue() {
    let Some(_env) = config::testenv::isolated() else {
        return;
    };
    let cfg = Config {
        provider: "apple".into(),
        user_token: "account-one".into(),
        ..Default::default()
    };
    cfg.save().unwrap();
    let d = daemon(None);
    let lib = d.library();
    lib.set_items(
        ipc::LIST_ALBUMS,
        vec![Item {
            id: "unsaved".into(),
            ..Default::default()
        }],
    );
    let engine = Engine::stand_in();
    {
        let mut i = d.lock();
        i.eng = Some(engine.clone());
        i.resume = Some(Resume {
            ids: vec!["song".into()],
            id: "song".into(),
            pos: 42.0,
            ..Default::default()
        });
    }
    assert!(
        d.replace_session(&cfg, Arc::new(Fixture::default()))
            .is_none()
    );
    assert!(Arc::ptr_eq(&lib, &d.library()));
    assert_eq!(
        d.library().items(ipc::LIST_ALBUMS).unwrap()[0].id,
        "unsaved"
    );
    assert_eq!(d.lock().resume.as_ref().unwrap().pos, 42.0);
    assert!(same(&d.lock().eng, &engine));
}

#[tokio::test]
async fn a_new_account_detaches_the_old_player_and_saves_its_queue() {
    let Some(_env) = config::testenv::isolated() else {
        return;
    };
    let mut cfg = Config {
        provider: "apple".into(),
        user_token: "account-one".into(),
        ..Default::default()
    };
    cfg.save().unwrap();
    let old_dir = config::session_cache_dir();
    let d = daemon(None);
    let old_api = d.client().unwrap();
    let old_engine = Engine::stand_in();
    {
        let mut i = d.lock();
        i.eng = Some(old_engine.clone());
        i.resume = Some(Resume {
            ids: vec!["old-song".into()],
            id: "old-song".into(),
            pos: 42.0,
            ..Default::default()
        });
    }
    cfg.user_token = "account-two".into();
    cfg.save().unwrap();
    let new_dir = config::session_cache_dir();
    let mut queue = Resume::new_at(new_dir.clone());
    queue.id = "new-song".into();
    queue.ids = vec![queue.id.clone()];
    queue.save();
    let removed = d
        .replace_session(&cfg, Arc::new(Fixture::default()))
        .unwrap();
    assert!(Arc::ptr_eq(&removed, &old_engine));
    assert!(d.engine().is_none());
    assert_eq!(Resume::load_at(old_dir).unwrap().id, "old-song");
    assert_eq!(d.lock().resume.as_ref().unwrap().id, "new-song");
    assert!(
        !d.apply(
            &old_engine,
            Player {
                id: "old-song".into(),
                title: "Old song".into(),
                ..Default::default()
            }
        )
        .await
    );
    assert_eq!(d.lock().resume.as_ref().unwrap().id, "new-song");
    assert_eq!(Resume::load_at(new_dir).unwrap().id, "new-song");
    // A Chrome startup which began before the login cannot take over later.
    assert!(d.install_engine(&old_api, &old_engine).is_err());
    assert!(d.engine().is_none());
}

#[tokio::test]
async fn a_reply_from_before_login_cannot_populate_the_new_library() {
    let d = daemon(None);
    let gate = Arc::new((Notify::new(), Notify::new()));
    let old = Arc::new(ProviderSession {
        api: Arc::new(Fixture {
            playlists_gate: Some(gate.clone()),
            ..Default::default()
        }),
        lib: d.library(),
    });
    d.lock().api = Some(old.clone());
    let request = {
        let d = d.clone();
        tokio::spawn(async move { d.list(ipc::LIST_PLAYLISTS, &mut Message::default()).await })
    };
    gate.0.notified().await;
    let new = Arc::new(ProviderSession {
        api: Arc::new(Fixture::default()),
        lib: Arc::new(Library::default()),
    });
    {
        let mut i = d.lock();
        i.lib = new.lib.clone();
        i.api = Some(new.clone());
        i.state.status = ipc::STATUS_READY.into();
    }
    gate.1.notify_one();
    assert!(request.await.unwrap().is_err());
    assert!(old.lib.items(ipc::LIST_PLAYLISTS).is_none());
    assert!(new.lib.items(ipc::LIST_PLAYLISTS).is_none());
    // A delayed 401 from the old client must not log the new session out.
    assert!(
        d.auth_check::<()>(&old, Err(Error::Unauthorized))
            .await
            .is_err()
    );
    assert_eq!(d.lock().state.status, ipc::STATUS_READY);
    let mut reply = Message::default();
    d.list(ipc::LIST_PLAYLISTS, &mut reply).await.unwrap();
    assert_eq!(reply.items[0].id, "fixture-playlist");
}
