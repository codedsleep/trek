//! Controlled copy benchmark on owned temporary trees: /tmp → fixture btrfs.
use gio::prelude::*;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};
use tallboy_fs::{guard::Policy, jobs::*};
fn dataset(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![0u8; 1024 * 1024];
    let mut state = 0x123456789abcdef0u64;
    for byte in &mut data {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        *byte = state as u8;
    }
    let mut file = fs::File::create(root.join("large.bin"))?;
    for _ in 0..1024 {
        file.write_all(&data)?;
    }
    for n in 0..128 {
        fs::write(root.join(format!("small-{n:03}")), &data[..65536])?;
    }
    file.sync_all()?;
    Ok(())
}
fn cp(src: &Path, dest: &Path) -> Result<Duration, Box<dyn std::error::Error>> {
    let start = Instant::now();
    let status = Command::new("cp")
        .args(["-a", "--"])
        .arg(src)
        .arg(dest)
        .status()?;
    if !status.success() {
        return Err("cp -a failed".into());
    }
    Ok(start.elapsed())
}
fn job(src: &Path, dest: &Path) -> Result<Duration, Box<dyn std::error::Error>> {
    let start = Instant::now();
    let policy = Policy {
        roots: vec![src.into(), dest.into()],
        uri_roots: vec![],
        data_home: dest.join("data"),
        unlocked: false,
    };
    let h = spawn_with_policy(
        JobKind::Copy {
            sources: vec![gio::File::for_path(src).uri().into()],
            dest_dir: gio::File::for_path(dest).uri().into(),
        },
        policy,
    );
    loop {
        if let JobEvent::Finished(s) = h.events.recv_blocking()? {
            if s.cancelled || !s.errors.is_empty() {
                return Err(format!("Copy benchmark: {s:?}").into());
            }
            return Ok(start.elapsed());
        }
        if start.elapsed() > Duration::from_secs(60) {
            h.cancel();
            return Err("Copy benchmark timed out".into());
        }
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = glib::user_cache_dir().join("tallboy-fixture");
    fs::create_dir_all(&fixture)?;
    let source = tempfile::tempdir_in("/tmp")?;
    dataset(source.path())?;
    let output = tempfile::tempdir_in(&fixture)?;
    let mut cp_times = vec![];
    let mut job_times = vec![];
    for n in 0..3 {
        let cp_dest = output.path().join(format!("cp-{n}"));
        let job_dest = output.path().join(format!("job-{n}"));
        fs::create_dir(&cp_dest)?;
        fs::create_dir(&job_dest)?;
        let (cp_time, job_time) = if n % 2 == 0 {
            (cp(source.path(), &cp_dest)?, job(source.path(), &job_dest)?)
        } else {
            let j = job(source.path(), &job_dest)?;
            (cp(source.path(), &cp_dest)?, j)
        };
        println!(
            "run {}: cp -a {:.3} ms; job {:.3} ms",
            n + 1,
            cp_time.as_secs_f64() * 1000.0,
            job_time.as_secs_f64() * 1000.0
        );
        cp_times.push(cp_time);
        job_times.push(job_time);
        fs::remove_dir_all(cp_dest)?;
        fs::remove_dir_all(job_dest)?;
    }
    cp_times.sort();
    job_times.sort();
    let ratio = job_times[1].as_secs_f64() / cp_times[1].as_secs_f64();
    println!(
        "cross-filesystem median: cp {:.3} ms; job {:.3} ms; ratio {:.3} (budget <= 1.100)",
        cp_times[1].as_secs_f64() * 1000.0,
        job_times[1].as_secs_f64() * 1000.0,
        ratio
    );
    let native = output.path().join("native");
    fs::create_dir(&native)?;
    cp(source.path(), &native)?;
    let native_src = native.join(source.path().file_name().ok_or("Source has no name")?);
    let same = output.path().join("same");
    fs::create_dir(&same)?;
    let duration = job(&native_src, &same)?;
    let file: PathBuf = same
        .join(native_src.file_name().ok_or("Source has no name")?)
        .join("large.bin");
    fs::File::open(native_src.join("large.bin"))?.sync_all()?;
    fs::File::open(&file)?.sync_all()?;
    let frag = Command::new("filefrag").arg("-v").arg(file).output()?;
    let cloned = String::from_utf8_lossy(&frag.stdout).contains("shared");
    println!(
        "same filesystem: {:.3} ms; cloned extents: {cloned}",
        duration.as_secs_f64() * 1000.0
    );
    // The first clone may flush the newly copied source's dirty extents.
    // Compare warm, synced native copies separately from that first-use cost.
    let mut native_cp = vec![];
    let mut native_job = vec![];
    for n in 0..3 {
        let cp_dest = output.path().join(format!("native-cp-{n}"));
        let job_dest = output.path().join(format!("native-job-{n}"));
        fs::create_dir(&cp_dest)?;
        fs::create_dir(&job_dest)?;
        let (c, j) = if n % 2 == 0 {
            (cp(&native_src, &cp_dest)?, job(&native_src, &job_dest)?)
        } else {
            let j = job(&native_src, &job_dest)?;
            (cp(&native_src, &cp_dest)?, j)
        };
        native_cp.push(c);
        native_job.push(j);
        fs::remove_dir_all(cp_dest)?;
        fs::remove_dir_all(job_dest)?;
    }
    native_cp.sort();
    native_job.sort();
    println!(
        "same-filesystem warm median: cp {:.3} ms; job {:.3} ms; ratio {:.3} (budget <= 1.100)",
        native_cp[1].as_secs_f64() * 1000.0,
        native_job[1].as_secs_f64() * 1000.0,
        native_job[1].as_secs_f64() / native_cp[1].as_secs_f64(),
    );
    if ratio > 1.1 {
        return Err("Cross-filesystem throughput budget exceeded".into());
    }
    Ok(())
}
