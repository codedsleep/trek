#![allow(dead_code)]
use gio::prelude::*;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use trek_fs::{guard::Policy, jobs::*};
pub fn temp() -> tempfile::TempDir {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    fs::create_dir_all(&root).unwrap();
    tempfile::tempdir_in(root).unwrap()
}
pub fn uri(path: impl AsRef<Path>) -> String {
    gio::File::for_path(path).uri().into()
}
pub fn policy(root: &Path) -> Policy {
    Policy {
        roots: vec![root.into()],
        uri_roots: vec![],
        data_home: root.join("data"),
        unlocked: false,
    }
}
pub fn finish(handle: &JobHandle, mut respond: impl FnMut(Question) -> Answer) -> JobSummary {
    drive(handle, |event| {
        if let JobEvent::Ask(q, reply) = event {
            reply.answer(respond(q)).unwrap();
        }
        None
    })
}
pub fn drive(
    handle: &JobHandle,
    mut event: impl FnMut(JobEvent) -> Option<JobSummary>,
) -> JobSummary {
    let start = Instant::now();
    loop {
        match handle.events.try_recv() {
            Ok(JobEvent::Finished(s)) => return s,
            Ok(e) => {
                if let Some(s) = event(e) {
                    return s;
                }
            }
            Err(async_channel::TryRecvError::Empty) => {
                assert!(start.elapsed() < Duration::from_secs(15), "job timed out");
                std::thread::sleep(Duration::from_millis(2));
            }
            Err(e) => panic!("job channel closed without Finished: {e}"),
        }
    }
}
pub fn success(kind: JobKind, root: &Path) -> JobSummary {
    let h = spawn_with_policy(kind, policy(root));
    let s = finish(&h, |q| panic!("Unexpected question: {q:?}"));
    assert!(!s.cancelled);
    assert!(s.errors.is_empty(), "{s:?}");
    s
}
