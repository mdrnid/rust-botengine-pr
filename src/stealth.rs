use crate::cdp::CdpClient;
use anyhow::Result;
use serde_json::json;

const STEALTH_JS: &str = r#"
(() => {
    // 1. Hide navigator.webdriver (primary CDP detection signal)
    Object.defineProperty(navigator, 'webdriver', {
        get: () => false,
        configurable: true,
    });

    // 2. Fix navigator.plugins (headless/CDP Chrome has 0 plugins)
    Object.defineProperty(navigator, 'plugins', {
        get: () => {
            const p = {
                length: 5,
                item: (i) => ({ name: 'Plugin ' + i }),
                namedItem: () => null,
                refresh: () => {},
                [Symbol.iterator]: function*() {
                    for (let i = 0; i < 5; i++) yield { name: 'Plugin ' + i };
                },
            };
            return p;
        },
        configurable: true,
    });

    // 3. Fix navigator.mimeTypes
    Object.defineProperty(navigator, 'mimeTypes', {
        get: () => ({
            length: 4,
            item: (i) => ({ type: 'application/pdf' }),
            namedItem: () => null,
            [Symbol.iterator]: function*() {
                for (let i = 0; i < 4; i++) yield { type: 'application/pdf' };
            },
        }),
        configurable: true,
    });

    // 4. Fix navigator.languages
    Object.defineProperty(navigator, 'languages', {
        get: () => ['en-US', 'en'],
        configurable: true,
    });

    // 5. Ensure chrome.runtime exists (undefined = CDP artifact)
    if (!window.chrome) window.chrome = {};
    if (!window.chrome.runtime) {
        window.chrome.runtime = {
            connect: function() {},
            sendMessage: function() {},
            onMessage: { addListener: function() {} },
        };
    }

    // 6. Patch Permissions API (CDP breaks notifications query)
    try {
        const origQuery = window.Permissions && Permissions.prototype.query;
        if (origQuery) {
            Permissions.prototype.query = function(parameters) {
                if (parameters.name === 'notifications') {
                    return Promise.resolve({ state: Notification.permission });
                }
                return origQuery.call(this, parameters);
            };
        }
    } catch (_) {}

    // 7. Fix window.outerWidth/outerHeight (0 in some CDP modes)
    if (window.outerWidth === 0) {
        Object.defineProperty(window, 'outerWidth', { get: () => window.innerWidth });
    }
    if (window.outerHeight === 0) {
        Object.defineProperty(window, 'outerHeight', { get: () => window.innerHeight + 85 });
    }

    // 8. Remove CDP-specific error stack traces
    const origToString = Error.prototype.toString;
    Error.prototype.toString = function() {
        const s = origToString.call(this);
        return s.replace(/chrome-devtools:\/\/|devtools:\/\//g, '');
    };

    // 9. Fix navigator.connection (missing in some CDP setups)
    if (!navigator.connection) {
        Object.defineProperty(navigator, 'connection', {
            get: () => ({
                effectiveType: '4g',
                rtt: 50,
                downlink: 10,
                saveData: false,
            }),
            configurable: true,
        });
    }

    // 10. Prevent toString detection of patched functions
    const nativeToString = Function.prototype.toString;
    const patchedFns = new Set();
    const handler = {
        apply: function(target, thisArg, args) {
            if (patchedFns.has(thisArg)) {
                return 'function ' + (thisArg.name || '') + '() { [native code] }';
            }
            return nativeToString.apply(thisArg, args);
        },
    };
    Function.prototype.toString = new Proxy(nativeToString, handler);
    patchedFns.add(Function.prototype.toString);
})();
"#;

/// Inject anti-detection scripts into a browser tab.
/// Uses Page.addScriptToEvaluateOnNewDocument for future navigations
/// and Runtime.evaluate for the current page.
pub async fn inject_stealth(cdp: &CdpClient) -> Result<()> {
    cdp.command(
        "Page.addScriptToEvaluateOnNewDocument",
        json!({ "source": STEALTH_JS }),
    )
    .await?;

    // Patch the already-loaded page too (best-effort)
    let _ = cdp.evaluate(STEALTH_JS).await;

    Ok(())
}

