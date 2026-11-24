use crate::browser::Browser;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewportInfo {
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrollInfo {
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageInfo {
    pub title: String,
    pub url: String,
    #[serde(rename = "readyState")]
    pub ready_state: String,
    pub viewport: ViewportInfo,
    pub scroll: ScrollInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementInfo {
    pub index: usize,
    pub tag: String,
    pub text: String,
    pub role: Option<String>,
    pub id: Option<String>,
    pub class: Option<String>,
    #[serde(rename = "ariaLabel")]
    pub aria_label: Option<String>,
    pub placeholder: Option<String>,
    pub name: Option<String>,
    #[serde(rename = "type")]
    pub element_type: Option<String>,
    pub href: Option<String>,
    pub disabled: bool,
    pub visible: bool,
    pub rect: Rect,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectResult {
    pub page: PageInfo,
    pub elements: Vec<ElementInfo>,
}

#[derive(Debug, Clone, Default)]
pub struct InspectOptions {
    pub selector: Option<String>,
    pub all: bool,
    pub visible_only: bool,
    pub filter: Option<String>,
    pub limit: Option<usize>,
    pub index: Option<usize>,
}

const INSPECT_FUNCTION_BODY: &str = r#"
(customSelector, allDom) => {
    try {
        const isVisible = (el, rect, style) => {
            if (!rect || rect.width <= 0 || rect.height <= 0) return false;
            if (style.display === 'none') return false;
            if (style.visibility === 'hidden' || style.visibility === 'collapse') return false;
            if (parseFloat(style.opacity || '1') === 0) return false;
            if (el.hidden || (el.closest && el.closest('[hidden]'))) return false;
            return true;
        };

        const getRole = (el) => {
            const explicit = el.getAttribute('role');
            if (explicit && explicit.trim()) return explicit.trim();
            const tag = el.tagName.toLowerCase();
            if (tag === 'button') return 'button';
            if (tag === 'a' && el.hasAttribute('href')) return 'link';
            if (tag === 'textarea') return 'textbox';
            if (tag === 'select') return 'combobox';
            if (tag === 'summary') return 'button';
            if (tag === 'input') {
                const t = (el.type || 'text').toLowerCase();
                if (['button', 'submit', 'reset'].includes(t)) return 'button';
                if (t === 'checkbox') return 'checkbox';
                if (t === 'radio') return 'radio';
                if (t === 'search') return 'searchbox';
                return 'textbox';
            }
            return null;
        };

        const getElementText = (el) => {
            const tag = el.tagName.toLowerCase();
            if (tag === 'input') {
                const type = (el.type || '').toLowerCase();
                if (['button', 'submit', 'reset'].includes(type)) {
                    return (el.value || '').trim();
                }
                return (el.value || el.placeholder || el.getAttribute('aria-label') || '').trim();
            }
            if (tag === 'select') {
                if (el.selectedOptions && el.selectedOptions.length > 0) {
                    return (el.selectedOptions[0].textContent || '').trim();
                }
                return (el.value || '').trim();
            }
            let text = (el.innerText || el.textContent || '').trim().replace(/\s+/g, ' ');
            if (!text) {
                const childIcon = el.querySelector('[aria-label], [title], [alt]');
                if (childIcon) {
                    text = (childIcon.getAttribute('aria-label') || childIcon.getAttribute('title') || childIcon.getAttribute('alt') || '').trim();
                } else {
                    text = (el.getAttribute('aria-label') || el.getAttribute('title') || el.getAttribute('alt') || '').trim();
                }
            }
            if (text.length > 300) {
                text = text.substring(0, 297) + '...';
            }
            return text;
        };

        const page = {
            title: document.title || '',
            url: window.location.href,
            readyState: document.readyState,
            viewport: {
                width: window.innerWidth,
                height: window.innerHeight
            },
            scroll: {
                width: Math.max(
                    document.documentElement ? document.documentElement.scrollWidth : 0,
                    document.body ? document.body.scrollWidth : 0
                ),
                height: Math.max(
                    document.documentElement ? document.documentElement.scrollHeight : 0,
                    document.body ? document.body.scrollHeight : 0
                )
            }
        };

        const interactiveSelectors = [
            'button',
            'a[href]',
            'input:not([type="hidden"])',
            'textarea',
            'select',
            'summary',
            '[role="button"]',
            '[role="link"]',
            '[role="checkbox"]',
            '[role="radio"]',
            '[role="switch"]',
            '[role="tab"]',
            '[role="menuitem"]',
            '[role="menuitemcheckbox"]',
            '[role="menuitemradio"]',
            '[role="combobox"]',
            '[role="textbox"]',
            '[role="searchbox"]',
            '[contenteditable="true"]',
            '[contenteditable=""]',
            '[tabindex]:not([tabindex="-1"])'
        ];

        let candidateElements = [];
        if (customSelector) {
            candidateElements = Array.from(document.querySelectorAll(customSelector));
        } else if (allDom) {
            candidateElements = Array.from(document.querySelectorAll('*'));
        } else {
            const set = new Set();
            for (const sel of interactiveSelectors) {
                const nodes = document.querySelectorAll(sel);
                for (const node of nodes) {
                    set.add(node);
                }
            }

            const clickables = document.querySelectorAll('[onclick], [data-action], [role]');
            for (const node of clickables) {
                set.add(node);
            }

            candidateElements = Array.from(set);
        }

        const elements = [];
        let index = 0;

        for (const el of candidateElements) {
            if (
                el === document.documentElement ||
                el === document.body ||
                el.tagName === 'SCRIPT' ||
                el.tagName === 'STYLE' ||
                el.tagName === 'NOSCRIPT' ||
                el.tagName === 'HEAD' ||
                el.tagName === 'META' ||
                el.tagName === 'LINK'
            ) {
                continue;
            }

            const rectObj = el.getBoundingClientRect();
            const style = window.getComputedStyle(el);
            const visible = isVisible(el, rectObj, style);

            const tag = el.tagName.toLowerCase();
            const text = getElementText(el);
            const role = getRole(el);
            const id = el.id && el.id.trim() ? el.id.trim() : null;

            let className = null;
            if (typeof el.className === 'string') {
                className = el.className.trim() || null;
            } else if (el.className && typeof el.className.baseVal === 'string') {
                className = el.className.baseVal.trim() || null;
            }

            let ariaLabel = (el.getAttribute('aria-label') || '').trim() || null;
            if (!ariaLabel) {
                const childWithAria = el.querySelector('[aria-label]');
                if (childWithAria) {
                    ariaLabel = (childWithAria.getAttribute('aria-label') || '').trim() || null;
                }
            }

            const placeholder = (el.getAttribute('placeholder') || '').trim() || null;
            const name = (el.getAttribute('name') || '').trim() || null;

            let type = null;
            if (tag === 'input') {
                type = (el.type || el.getAttribute('type') || 'text').toLowerCase();
            } else if (tag === 'button') {
                type = (el.type || el.getAttribute('type') || 'submit').toLowerCase();
            } else {
                type = el.getAttribute('type') ? el.getAttribute('type').toLowerCase() : null;
            }

            let href = null;
            if (tag === 'a') {
                href = el.getAttribute('href') || el.href || null;
            } else if (el.hasAttribute('href')) {
                href = el.getAttribute('href') || null;
            }

            const disabled = Boolean(el.disabled || el.hasAttribute('disabled') || el.getAttribute('aria-disabled') === 'true');

            const rect = {
                x: Math.round(rectObj.x * 100) / 100,
                y: Math.round(rectObj.y * 100) / 100,
                width: Math.round(rectObj.width * 100) / 100,
                height: Math.round(rectObj.height * 100) / 100
            };

            let value = null;
            if (['input', 'textarea', 'select'].includes(tag)) {
                value = el.value || null;
            }

            let checked = null;
            if (tag === 'input' && (type === 'checkbox' || type === 'radio')) {
                checked = Boolean(el.checked);
            }

            elements.push({
                index: index++,
                tag: tag,
                text: text,
                role: role,
                id: id,
                class: className,
                ariaLabel: ariaLabel,
                placeholder: placeholder,
                name: name,
                type: type,
                href: href,
                disabled: disabled,
                visible: visible,
                rect: rect,
                value: value,
                checked: checked
            });
        }

        return JSON.stringify({
            page: page,
            elements: elements
        });
    } catch (err) {
        return JSON.stringify({
            error: err.message || String(err)
        });
    }
}
"#;

pub async fn inspect(browser: &Browser, options: &InspectOptions) -> Result<InspectResult> {
    let selector_arg = match &options.selector {
        Some(s) => serde_json::to_string(s)?,
        None => "null".to_string(),
    };
    let all_dom_arg = if options.all { "true" } else { "false" };

    let full_expression = format!("({INSPECT_FUNCTION_BODY})({selector_arg}, {all_dom_arg});");
    let raw_val = browser.evaluate(&full_expression).await?;

    let json_str = if let Some(s) = raw_val.as_str() {
        s.to_string()
    } else if raw_val.is_object() {
        raw_val.to_string()
    } else {
        anyhow::bail!("Unexpected response from inspect evaluation: {raw_val}");
    };

    let mut result: InspectResult = serde_json::from_str(&json_str)
        .with_context(|| format!("Failed to parse DOM inspect result: {json_str}"))?;

    // Apply filtering options in Rust for precision and flexibility
    if options.visible_only {
        result.elements.retain(|el| el.visible);
    }

    if let Some(target_idx) = options.index {
        result.elements.retain(|el| el.index == target_idx);
    }

    if let Some(ref f) = options.filter {
        let f_lower = f.to_lowercase();
        result.elements.retain(|el| {
            el.text.to_lowercase().contains(&f_lower)
                || el.tag.to_lowercase().contains(&f_lower)
                || el.id.as_deref().map_or(false, |id| id.to_lowercase().contains(&f_lower))
                || el.class.as_deref().map_or(false, |c| c.to_lowercase().contains(&f_lower))
                || el.aria_label.as_deref().map_or(false, |a| a.to_lowercase().contains(&f_lower))
                || el.name.as_deref().map_or(false, |n| n.to_lowercase().contains(&f_lower))
                || el.placeholder.as_deref().map_or(false, |p| p.to_lowercase().contains(&f_lower))
                || el.role.as_deref().map_or(false, |r| r.to_lowercase().contains(&f_lower))
        });
    }

    if let Some(limit) = options.limit {
        result.elements.truncate(limit);
    }

    // Re-index elements sequentially if filtering was applied so consumers have a clean 0..N sequence
    if options.visible_only || options.index.is_some() || options.filter.is_some() || options.limit.is_some() {
        for (i, el) in result.elements.iter_mut().enumerate() {
            el.index = i;
        }
    }

    Ok(result)
}
