use super::*;
use crate::provider::tests::Fixture;

fn daemon(failure: Option<Error>) -> Arc<Daemon> {
    let d = Arc::new(Daemon::new("dev", Library::default(), None));
    d.lock().api = Some(Arc::new(Fixture { failure }));
    d
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
    d.fetch_list(api.as_ref(), ipc::LIST_PLAYLISTS, &mut reply)
        .await
        .unwrap();
    assert_eq!(reply.items.len(), 1);
    assert!(d.lib.items(ipc::LIST_PLAYLISTS).is_none());
    let item = Item {
        kind: KIND_ALBUM.into(),
        id: "fixture-album".into(),
        ..Default::default()
    };
    d.open(&item, &mut reply).await.unwrap();
    assert_eq!(reply.tracks.len(), 1);
    assert!(d.lib.tracks(&item.key()).is_none());
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
