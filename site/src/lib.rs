//! trafford's documentation and landing page: the generator, and the server
//! that serves it while you write.
//!
//! Not a site generator. It renders one project's docs, which is what keeps it
//! small enough to own outright — and it renders them through the app's own
//! scanners, so a `[[wikilink]]` on the website means what it means in the
//! editor. `cairn/items/0064` is why that decided the shape of all of it.

pub mod build;
pub mod html;
pub mod keys;
pub mod palette;
pub mod serve;
pub mod shell;
pub mod watch;
