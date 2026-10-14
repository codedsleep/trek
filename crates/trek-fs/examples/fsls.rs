//! Headless listing benchmark: cargo run --release -p trek-fs --example fsls -- PATH.
use std::{path::Path, time::Instant};
use trek_fs::{
    folder::{Folder, FolderEvent, Options},
    meta,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arg = std::env::args().nth(1).ok_or("Usage: fsls PATH_OR_URI")?;
    let location =
        trek_core::location::Location::parse(&arg, &std::env::current_dir()?, Path::new("/"))?;
    let ctx = glib::MainContext::default();
    ctx.with_thread_default(||ctx.block_on(async{let start=Instant::now();let folder=Folder::new(&gio::File::for_uri(location.to_uri()),Options::default(),None)?;let mut items=Vec::new();let mut first=None;loop{match folder.events.recv().await?{FolderEvent::Batch(batch)=>{if first.is_none(){first=Some(start.elapsed());}items.extend(batch);},FolderEvent::Error(e)=>return Err::<(),Box<dyn std::error::Error>>(e.into()),FolderEvent::Finished=>break,_=>{}}}let load=start.elapsed();let sort_start=Instant::now();items.sort_by(|a,b|meta::compare(a,b,&Default::default()));println!("{} entries; first batch {:.3} ms; loaded {:.3} ms; sorted {:.3} ms; load+sort {:.3} ms",items.len(),first.unwrap_or_default().as_secs_f64()*1000.0,load.as_secs_f64()*1000.0,sort_start.elapsed().as_secs_f64()*1000.0,start.elapsed().as_secs_f64()*1000.0);Ok(())}))?
}
