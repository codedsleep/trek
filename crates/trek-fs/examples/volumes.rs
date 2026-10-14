//! Read-only inventory: cargo run -p trek-fs --example volumes.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ctx = glib::MainContext::default();
    ctx.with_thread_default(|| {
        ctx.block_on(async {
            let model = trek_fs::volumes::Model::new(trek_fs::volumes::Filters {
                places_unmounted: true,
                show_internal_volumes: true,
            });
            glib::timeout_future(std::time::Duration::from_millis(300)).await;
            for entry in model.snapshot() {
                println!(
                    "{}: {} device={:?} mounted={} removable={} internal={} root={:?}",
                    entry.id,
                    entry.name,
                    entry.device,
                    entry.mounted,
                    entry.removable,
                    entry.internal,
                    entry.root
                );
            }
            Ok(())
        })
    })?
}
