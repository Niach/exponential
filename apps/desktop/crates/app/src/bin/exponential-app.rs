//! `exponential-app` (#EDE-227): Exponential Desktop's component, the app as
//! a part of the session. The compositor starts it as one of its helpers
//! (so it is known to it: no consent prompt, launch tokens over the session
//! bus) and starts it again when it ends; it holds the login's account,
//! install, keys, device and coding sessions in the app's own data
//! directory. Everything it does is [`app::run`] in [`app::Mode::Component`].

fn main() {
    app::run(app::Mode::Component);
}
