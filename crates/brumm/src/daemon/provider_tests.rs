use super::*;
use crate::provider::tests::Fixture;

fn daemon(failure: Option<Error>) -> Arc<Daemon> {
    let d = Arc::new(Daemon::new("dev", Library::default(), None));
    d.lock().api = Some(Arc::new(Fixture { failure }));
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

/// While a renewal of the page's token is under way, more refusals from the
/// page say "renewing", not "signed out".
#[tokio::test]
async fn a_renewal_under_way_is_not_a_sign_out() {
    let d = daemon(None);
    {
        let mut i = d.lock();
        i.eng = Some(crate::engine::Engine::stand_in());
        i.session = Some(Arc::new(crate::provider::spotify::Session::stale(
            "http://127.0.0.1:9",
            "old",
        )));
    }
    assert!(d.renew_token(), "a renewal starts");
    assert!(d.renew_token(), "and is still under way");
    d.lock().renewing = false;
    d.lock().renewed_at = Some(Instant::now() - Duration::from_secs(30));
    assert!(
        !d.renew_token(),
        "refused again right after one: signed out"
    );
}

#[test]
fn switching_services_swaps_the_player_library_and_place() {
    let Some(_env) = crate::config::testenv::isolated() else {
        return;
    };
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let d = daemon(None);
        d.lock().provider = "apple".into();
        d.lock().booted = true;
        d.lib.set_items(ipc::LIST_PLAYLISTS, vec![Item::default()]);
        d.lock().resume = Some(Resume {
            ids: vec!["a".into()],
            id: "a".into(),
            title: "Apple song".into(),
            ..Resume::new("apple")
        });
        let mut reply = Message::default();
        d.select_provider("spotify", &mut reply).await.unwrap();
        assert_eq!(reply.provider.unwrap().selected, "spotify");
        {
            let i = d.lock();
            assert_eq!(i.provider, "spotify");
            assert!(i.api.is_none() && !i.booted && i.resume.is_none());
        }
        assert!(
            d.lib.items(ipc::LIST_PLAYLISTS).is_none(),
            "Spotify's own cache"
        );
        // Back to Apple: its library and place are still there.
        d.select_provider("apple", &mut Message::default())
            .await
            .unwrap();
        assert_eq!(d.lib.items(ipc::LIST_PLAYLISTS).unwrap().len(), 1);
        assert_eq!(d.lock().resume.as_ref().unwrap().title, "Apple song");
        assert!(
            d.select_provider("tidal", &mut Message::default())
                .await
                .is_err()
        );
        assert_eq!(d.lock().provider, "apple");
    });
}
