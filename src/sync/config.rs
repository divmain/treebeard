use crate::config::{load_config, save_config, Config};
use crate::error::Result;

fn save_config_pattern<F>(pattern: String, field_accessor: F) -> Result<()>
where
    F: FnOnce(&mut Config) -> &mut Vec<String>,
{
    let mut config = load_config()?;
    let field = field_accessor(&mut config);
    if !field.contains(&pattern) {
        field.push(pattern);
        save_config(&config)?;
    }
    Ok(())
}

pub fn save_skip_pattern(pattern: String) -> Result<()> {
    save_config_pattern(pattern, |c| &mut c.sync.sync_always_skip)
}

pub fn save_include_pattern(pattern: String) -> Result<()> {
    save_config_pattern(pattern, |c| &mut c.sync.sync_always_include)
}
