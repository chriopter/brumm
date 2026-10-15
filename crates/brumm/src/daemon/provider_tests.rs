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
