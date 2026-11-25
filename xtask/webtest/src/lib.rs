//! `lunaferra-webtest` will be the WebAssembly module that the browser test harness runs. It will
//! run the conformance and extension scripts through the API of the `lunaferra` crate in a browser,
//! and report each result to the page that loaded it, as the [quality document] describes.
//!
//! The crate holds no code yet. Task M3.17 of the [roadmap], which builds the test runners for
//! Android, iOS, and the web, starts it.
//!
//! [quality document]: https://github.com/jabbott-iii/Lunaferra/blob/main/docs/04-quality.md#lunaferra-webtest
//! [roadmap]: https://github.com/jabbott-iii/Lunaferra/blob/main/docs/roadmap.md
