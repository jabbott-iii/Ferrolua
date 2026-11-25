//! `ferrolua-webtest` will be the WebAssembly module that the browser test harness runs. It will
//! run the conformance and extension scripts through the API of the `ferrolua` crate in a browser,
//! and report each result to the page that loaded it, as the [quality document] describes.
//!
//! The crate holds no code yet. Task M3.17 of the [roadmap], which builds the test runners for
//! Android, iOS, and the web, starts it.
//!
//! [quality document]: https://github.com/jabbott-iii/Ferrolua/blob/main/docs/04-quality.md#ferrolua-webtest
//! [roadmap]: https://github.com/jabbott-iii/Ferrolua/blob/main/docs/roadmap.md
