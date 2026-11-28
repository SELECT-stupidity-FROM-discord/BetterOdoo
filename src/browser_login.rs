#[cfg(target_os = "macos")]
mod macos {
    use anyhow::{Result, anyhow};
    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject, NSObject, NSObjectProtocol};
    use objc2::{ClassType, MainThreadMarker, MainThreadOnly, define_class, msg_send};
    use objc2_app_kit::{NSBackingStoreType, NSView, NSWindow, NSWindowStyleMask};
    use objc2_foundation::{
        NSArray, NSHTTPCookie, NSPoint, NSRect, NSSize, NSString, NSURL, NSURLRequest, ns_string,
    };
    use std::ptr::NonNull;
    use std::sync::Mutex;
    use std::sync::mpsc::{self, Receiver};

    #[link(name = "WebKit", kind = "framework")]
    unsafe extern "C" {}

    define_class!(
        #[unsafe(super(NSObject))]
        #[name = "BetterOdooWebUIDelegate"]
        struct PopupDelegate;

        impl PopupDelegate {
            #[unsafe(method(webView:createWebViewWithConfiguration:forNavigationAction:windowFeatures:))]
            fn create_web_view(
                &self,
                web_view: &AnyObject,
                _config: &AnyObject,
                navigation_action: &AnyObject,
                _features: &AnyObject,
            ) -> *mut AnyObject {
                unsafe {
                    let target_frame: *const AnyObject = msg_send![navigation_action, targetFrame];
                    if target_frame.is_null() {
                        let request: *const AnyObject = msg_send![navigation_action, request];
                        if !request.is_null() {
                            let _: *mut AnyObject = msg_send![web_view, loadRequest: request];
                        }
                    }
                }
                std::ptr::null_mut()
            }
        }
    );

    unsafe impl NSObjectProtocol for PopupDelegate {}

    impl PopupDelegate {
        fn new() -> Retained<Self> {
            unsafe { msg_send![Self::class(), new] }
        }
    }

    pub struct BrowserWindow {
        window: Retained<NSWindow>,
        web_view: Retained<AnyObject>,
        _delegate: Retained<PopupDelegate>,
    }

    impl BrowserWindow {
        pub fn open(url: &str) -> Result<Self> {
            let mtm = MainThreadMarker::new()
                .ok_or_else(|| anyhow!("Browser sign-in must open on the main thread."))?;
            let ns_url = NSURL::URLWithString(&NSString::from_str(url))
                .ok_or_else(|| anyhow!("Invalid Odoo sign-in URL."))?;
            let request = NSURLRequest::requestWithURL(&ns_url);

            let config_cls = AnyClass::get(c"WKWebViewConfiguration")
                .ok_or_else(|| anyhow!("WebKit is unavailable on this system."))?;
            let store_cls = AnyClass::get(c"WKWebsiteDataStore")
                .ok_or_else(|| anyhow!("WebKit data store is unavailable."))?;
            let web_view_cls = AnyClass::get(c"WKWebView")
                .ok_or_else(|| anyhow!("WKWebView is unavailable on this system."))?;

            let frame = NSRect::new(NSPoint::new(0., 0.), NSSize::new(960., 720.));

            unsafe {
                let config: Retained<AnyObject> = msg_send![config_cls, new];
                let data_store: Retained<AnyObject> = msg_send![store_cls, nonPersistentDataStore];
                let _: () = msg_send![&*config, setWebsiteDataStore: &*data_store];

                let alloc: *mut AnyObject = msg_send![web_view_cls, alloc];
                let raw: *mut AnyObject =
                    msg_send![alloc, initWithFrame: frame, configuration: &*config];
                let web_view = Retained::from_raw(raw)
                    .ok_or_else(|| anyhow!("Couldn’t create browser view."))?;

                let delegate = PopupDelegate::new();
                let _: () = msg_send![&*web_view, setUIDelegate: &*delegate];

                let style = NSWindowStyleMask::Titled
                    | NSWindowStyleMask::Closable
                    | NSWindowStyleMask::Miniaturizable
                    | NSWindowStyleMask::Resizable;
                let window = NSWindow::initWithContentRect_styleMask_backing_defer(
                    NSWindow::alloc(mtm),
                    frame,
                    style,
                    NSBackingStoreType::Buffered,
                    false,
                );
                window.setReleasedWhenClosed(false);
                window.setTitle(ns_string!("Sign in to Odoo"));
                let view: &NSView = &*((&*web_view) as *const AnyObject as *const NSView);
                window.setContentView(Some(view));
                window.center();
                window.makeKeyAndOrderFront(None);

                let _: *mut AnyObject = msg_send![&*web_view, loadRequest: &*request];

                Ok(Self {
                    window,
                    web_view,
                    _delegate: delegate,
                })
            }
        }

        pub fn is_open(&self) -> bool {
            self.window.isVisible()
        }

        pub fn current_url(&self) -> Option<String> {
            unsafe {
                let url: Option<Retained<NSURL>> = msg_send![&*self.web_view, URL];
                url?.absoluteString().map(|s| s.to_string())
            }
        }

        pub fn request_session_cookie(&self, host: &str) -> Receiver<Option<String>> {
            let (tx, rx) = mpsc::sync_channel(1);
            let tx = Mutex::new(Some(tx));
            let target_host = host.to_string();

            let block = RcBlock::new(move |cookies: NonNull<NSArray<NSHTTPCookie>>| {
                let cookies = unsafe { cookies.as_ref() };
                let found = cookies.iter().find_map(|cookie| {
                    if cookie.name().to_string() != "session_id" {
                        return None;
                    }
                    let domain = cookie.domain().to_string();
                    if !super::cookie_matches_host(&domain, &target_host) {
                        return None;
                    }
                    let val = cookie.value().to_string();
                    (!val.is_empty()).then_some(val)
                });
                if let Ok(mut guard) = tx.lock()
                    && let Some(sender) = guard.take()
                {
                    let _ = sender.send(found);
                }
            });

            unsafe {
                let config: Retained<AnyObject> = msg_send![&*self.web_view, configuration];
                let data_store: Retained<AnyObject> = msg_send![&*config, websiteDataStore];
                let cookie_store: Retained<AnyObject> = msg_send![&*data_store, httpCookieStore];
                let _: () = msg_send![&*cookie_store, getAllCookies: &*block];
            }

            rx
        }

        pub fn close(&self) {
            unsafe {
                let nil: *const AnyObject = std::ptr::null();
                let _: () = msg_send![&*self.web_view, setUIDelegate: nil];
            }
            self.window.close();
        }
    }

    impl Drop for BrowserWindow {
        fn drop(&mut self) {
            self.close();
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod fallback {
    use anyhow::{Result, anyhow};
    use std::sync::mpsc::Receiver;

    pub struct BrowserWindow;

    impl BrowserWindow {
        pub fn open(_url: &str) -> Result<Self> {
            Err(anyhow!(
                "Browser window sign-in is currently available on macOS only."
            ))
        }

        pub fn is_open(&self) -> bool {
            false
        }

        pub fn current_url(&self) -> Option<String> {
            None
        }

        pub fn request_session_cookie(&self, _host: &str) -> Receiver<Option<String>> {
            let (_tx, rx) = std::sync::mpsc::sync_channel(1);
            rx
        }

        pub fn close(&self) {}
    }
}

#[cfg(target_os = "macos")]
pub use macos::BrowserWindow;

#[cfg(not(target_os = "macos"))]
pub use fallback::BrowserWindow;

pub const fn is_supported() -> bool {
    cfg!(target_os = "macos")
}

pub fn login_url(base_url: &str, db: &str) -> String {
    let base = base_url.trim_end_matches('/');
    if db.is_empty() {
        format!("{base}/web/login")
    } else {
        format!("{base}/web/login?db={}", encode_query(db))
    }
}

fn encode_query(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub fn url_host(url: &str) -> Option<String> {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next()?;
    let host_port = authority.rsplit('@').next()?;
    let host = host_port.split(':').next()?;
    (!host.is_empty()).then(|| host.to_lowercase())
}

fn url_path(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    match rest.find('/') {
        Some(i) => {
            let after = &rest[i..];
            after.split(['?', '#']).next().unwrap_or("/")
        }
        None => "/",
    }
}

pub fn cookie_matches_host(cookie_domain: &str, host: &str) -> bool {
    let domain = cookie_domain.trim().trim_start_matches('.').to_lowercase();
    let host = host.trim().to_lowercase();
    !domain.is_empty() && (host == domain || host.ends_with(&format!(".{domain}")))
}

pub fn is_post_login_url(current_url: &str, odoo_host: &str) -> bool {
    let Some(host) = url_host(current_url) else {
        return false;
    };
    if host != odoo_host.to_lowercase() {
        return false;
    }
    let path = url_path(current_url).to_lowercase();
    const AUTH_PATHS: [&str; 7] = [
        "/web/login",
        "/web/signup",
        "/web/reset_password",
        "/web/database",
        "/web/session/",
        "/auth_oauth",
        "/auth_saml",
    ];
    !AUTH_PATHS.iter().any(|p| path.starts_with(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_url_encodes_database_name() {
        assert_eq!(
            login_url("https://acme.odoo.com/", "acme-prod"),
            "https://acme.odoo.com/web/login?db=acme-prod"
        );
        assert_eq!(
            login_url("https://acme.odoo.com", "my db&1"),
            "https://acme.odoo.com/web/login?db=my%20db%261"
        );
        assert_eq!(
            login_url("https://acme.odoo.com", ""),
            "https://acme.odoo.com/web/login"
        );
    }

    #[test]
    fn host_and_cookie_domain_matching() {
        assert_eq!(
            url_host("https://Example.Odoo.com:443/web/login?db=a"),
            Some("example.odoo.com".into())
        );
        assert!(cookie_matches_host(".odoo.com", "acme.odoo.com"));
        assert!(cookie_matches_host("acme.odoo.com", "acme.odoo.com"));
        assert!(!cookie_matches_host("evil-odoo.com", "acme.odoo.com"));
        assert!(!cookie_matches_host("odoo.com", "evilodoo.com"));
    }

    #[test]
    fn post_login_url_filters_auth_pages() {
        assert!(!is_post_login_url(
            "https://acme.odoo.com/web/login?db=acme",
            "acme.odoo.com"
        ));
        assert!(!is_post_login_url(
            "https://acme.odoo.com/web/login/totp",
            "acme.odoo.com"
        ));
        assert!(!is_post_login_url(
            "https://login.microsoftonline.com/common/oauth2",
            "acme.odoo.com"
        ));
        assert!(!is_post_login_url(
            "https://acme.odoo.com/auth_oauth/signin#access_token=x",
            "acme.odoo.com"
        ));
        assert!(is_post_login_url(
            "https://acme.odoo.com/odoo/project",
            "acme.odoo.com"
        ));
        assert!(is_post_login_url(
            "https://acme.odoo.com/web#action=1",
            "acme.odoo.com"
        ));
        assert!(is_post_login_url(
            "https://acme.odoo.com/my/home",
            "acme.odoo.com"
        ));
    }
}
