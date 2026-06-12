pub mod java;
pub mod python;
pub mod typescript;

use crate::plugin::LanguagePlugin;
use once_cell::sync::Lazy;

static PLUGINS: Lazy<Vec<Box<dyn LanguagePlugin>>> = Lazy::new(|| {
    vec![
        Box::new(python::PythonPlugin),
        Box::new(typescript::TypeScriptPlugin { tsx: false }),
        Box::new(typescript::TypeScriptPlugin { tsx: true }),
        Box::new(java::JavaPlugin),
    ]
});

pub fn plugin_for(path: &str) -> Option<&'static dyn LanguagePlugin> {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{}", e.to_ascii_lowercase()))?;
    PLUGINS
        .iter()
        .find(|p| p.extensions().contains(&ext.as_str()))
        .map(|p| p.as_ref())
}
