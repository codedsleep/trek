//! `org.freedesktop.FileManager1` requests, without window or selection policy.
//!
//! Register only for the default profile on its main context. Retain `Service`
//! while handling its event receiver; dropping it unregisters and releases the name.
use glib::variant::ToVariant;
use std::{marker::PhantomData, path::Path, rc::Rc};
use tallboy_core::location::Location;

/// Well-known interface and bus name.
pub const NAME: &str = "org.freedesktop.FileManager1";
/// Standard object path.
pub const PATH: &str = "/org/freedesktop/FileManager1";
const XML: &str = r#"<node><interface name="org.freedesktop.FileManager1">
<method name="ShowFolders"><arg type="as" direction="in"/><arg type="s" direction="in"/></method>
<method name="ShowItems"><arg type="as" direction="in"/><arg type="s" direction="in"/></method>
<method name="ShowItemProperties"><arg type="as" direction="in"/><arg type="s" direction="in"/></method>
</interface></node>"#;

/// Validated activation request. Startup IDs are passed through to the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Open each folder.
    ShowFolders {
        /// Requested folders.
        locations: Vec<Location>,
        /// Caller activation token/startup ID.
        startup_id: String,
    },
    /// Open containing folders and select these items.
    ShowItems {
        /// Requested items.
        locations: Vec<Location>,
        /// Caller activation token/startup ID.
        startup_id: String,
    },
    /// Show Properties for the requested items.
    ShowItemProperties {
        /// Requested items.
        locations: Vec<Location>,
        /// Caller activation token/startup ID.
        startup_id: String,
    },
}
/// Registration failure; an existing file manager is never replaced or queued behind.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The name is already held, including on this connection.
    #[error("org.freedesktop.FileManager1 is already owned")]
    AlreadyOwned,
    /// D-Bus transport or object registration failed.
    #[error(transparent)]
    Dbus(#[from] glib::Error),
}
/// Main-context-bound object and name lifetime.
pub struct Service {
    connection: gio::DBusConnection,
    registration: Option<gio::RegistrationId>,
    /// Unbounded request stream; consume asynchronously on the UI context.
    pub events: async_channel::Receiver<Event>,
    context_bound: PhantomData<Rc<()>>,
}
impl Drop for Service {
    fn drop(&mut self) {
        self.events.close();
        if let Some(id) = self.registration.take() {
            let _ = self.connection.unregister_object(id);
        }
        release(&self.connection);
    }
}
fn release(connection: &gio::DBusConnection) {
    // Fire-and-forget: teardown must not block the UI or require polling a future.
    let message = gio::DBusMessage::new_method_call(
        Some("org.freedesktop.DBus"),
        "/org/freedesktop/DBus",
        Some("org.freedesktop.DBus"),
        "ReleaseName",
    );
    message.set_body(&(NAME,).to_variant());
    message.set_flags(gio::DBusMessageFlags::NO_REPLY_EXPECTED);
    let _ = connection.send_message(&message, gio::DBusSendMessageFlags::NONE);
}
/// Acquire without replacement (`DO_NOT_QUEUE`) and register on the caller's bus.
///
/// Poll on the main context that will dispatch the method calls. A name held by
/// another manager is a normal `AlreadyOwned` result, so the UI can continue.
pub async fn register(connection: &gio::DBusConnection) -> Result<Service, Error> {
    let node = gio::DBusNodeInfo::for_xml(XML)?;
    let interface = node
        .lookup_interface(NAME)
        .ok_or_else(|| glib::Error::new(gio::IOErrorEnum::InvalidData, "Missing interface"))?;
    let reply = connection
        .call_future(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "RequestName",
            Some(&(NAME, 4u32).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            5000,
        )
        .await?;
    if reply.get::<(u32,)>() != Some((1,)) {
        return Err(Error::AlreadyOwned);
    }
    let (tx, events) = async_channel::unbounded();
    let registration = connection
        .register_object(PATH, &interface)
        .method_call(move |_, _, _, _, method, parameters, invocation| {
            let Some((uris, startup_id)) = parameters.get::<(Vec<String>, String)>() else {
                invocation.return_dbus_error(
                    "org.freedesktop.DBus.Error.InvalidArgs",
                    "Expected URIs and startup ID",
                );
                return;
            };
            let locations = uris
                .iter()
                .map(|uri| {
                    if !uri.contains("://") {
                        return Err("FileManager1 requires absolute URIs".to_owned());
                    }
                    Location::parse(uri, Path::new("/"), Path::new("/"))
                        .map_err(|error| error.to_string())
                })
                .collect::<Result<Vec<_>, _>>();
            let locations = match locations {
                Ok(locations) => locations,
                Err(error) => {
                    invocation.return_dbus_error("org.freedesktop.DBus.Error.InvalidArgs", &error);
                    return;
                }
            };
            let event = match method {
                "ShowFolders" => Event::ShowFolders {
                    locations,
                    startup_id,
                },
                "ShowItems" => Event::ShowItems {
                    locations,
                    startup_id,
                },
                "ShowItemProperties" => Event::ShowItemProperties {
                    locations,
                    startup_id,
                },
                _ => {
                    invocation.return_dbus_error(
                        "org.freedesktop.DBus.Error.UnknownMethod",
                        "Unknown FileManager1 method",
                    );
                    return;
                }
            };
            if tx.try_send(event).is_err() {
                invocation.return_dbus_error(
                    "org.freedesktop.DBus.Error.Failed",
                    "File manager is shutting down",
                );
            } else {
                invocation.return_value(Some(&().to_variant()));
            }
        })
        .build();
    match registration {
        Ok(registration) => Ok(Service {
            connection: connection.clone(),
            registration: Some(registration),
            events,
            context_bound: PhantomData,
        }),
        Err(error) => {
            release(connection);
            Err(error.into())
        }
    }
}
