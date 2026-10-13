//! Opens brumm as a window: brumm-gui, a Qt Quick program installed next
//! to brumm (gui/ in the source), talking to the same daemon as the
//! terminal player.

use std::os::unix::process::CommandExt;

use crate::error::Result;
use crate::launch;

/// Starts the daemon if needed and becomes the window. The window itself
/// brings an open one to the front instead of opening another.
pub fn run(rt: &tokio::runtime::Runtime) -> Result<()> {
    let p = launch::gui_path()?;
    let (c, _) = rt.block_on(launch::connect())?;
    rt.block_on(c.close());
    let e = std::process::Command::new(p).arg0(launch::GUI_NAME).exec();
    Err(e.into())
}
