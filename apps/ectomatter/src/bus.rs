//! The session's D-Bus as Ectomatter's threads ask it, each call waiting for
//! its answer: zbus is async, and runs the bus's work between the polls of
//! each wait (zbus::block_on), so code that isn't async reaches the
//! desktop's services as it calls a function. A connection made here goes on
//! serving what it holds (KWin's script's listener, Ectomatter opened again)
//! after the call that made it returns, on a thread zbus keeps for it.
//! Not to be called from inside a served interface's method, which zbus is
//! already running (block_on panics there).


use serde::Serialize;
use serde::de::DeserializeOwned;
use zbus::wire::{DynamicType, OwnedVariant, Type};

pub use zbus::Connection;

/// The person's session bus.
pub fn session() -> zbus::Result<Connection> {
    zbus::block_on(Connection::session())
}

/// A method called on `dest`'s object at `path`, and its answer.
pub fn call<R, B>(conn: &Connection, dest: &str, path: &str, iface: &str, method: &str, body: &B) -> zbus::Result<R>
where
    R: DeserializeOwned + Type,
    B: Serialize + DynamicType,
{
    zbus::block_on(async {
        let reply = conn.call_method(Some(dest), path, Some(iface), method, body).await?;
        reply.body().deserialize::<R>()
    })
}

/// A property of `dest`'s object at `path`.
pub fn property<R>(conn: &Connection, dest: &str, path: &str, iface: &str, name: &str) -> zbus::Result<R>
where
    R: TryFrom<OwnedVariant>,
    R::Error: Into<zbus::Error>,
{
    let v: OwnedVariant = call(conn, dest, path, "org.freedesktop.DBus.Properties", "Get", &(iface, name))?;
    R::try_from(v).map_err(Into::into)
}

/// Whether something on the bus holds a name.
pub fn has_owner(conn: &Connection, name: &str) -> bool {
    call::<bool, _>(conn, "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "NameHasOwner", &(name,)).unwrap_or(false)
}

/// The next item of a stream (a signal's), for an async block here.
pub async fn next<S: futures_core::Stream + Unpin>(stream: &mut S) -> Option<S::Item> {
    std::future::poll_fn(|cx| std::pin::Pin::new(&mut *stream).poll_next(cx)).await
}
