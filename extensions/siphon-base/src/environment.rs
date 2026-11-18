//! Runtime facts read through Octoii's public interfaces and described to the model.
use octoii::agent::{Teller, Telling, Writable, WritableFolders};
use octoii::machine::Host;
use octoii::threads::Mode;
use octoii::tools::Place;
use octoii::volume::Volume;
use std::fmt::Write as _;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

#[derive(Debug)]
pub(crate) struct Environment {
    pub host: Host,
    pub writable: WritableFolders,
}

impl Teller for Environment {
    fn tell<'a>(
        &'a self,
        volume: &'a Volume,
        thread: &'a str,
        place: Option<&'a Place>,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Telling>> + Send + 'a>> {
        Box::pin(async move {
            let mode = volume.thread_config(thread)?.and_then(|config| config.mode).unwrap_or_default();
            let writable = self.writable.read(volume)?;
            let places = Places {
                scratchpad: self.host.scratchpad_of(thread),
                temporary: &self.host.temporary,
                writable: &writable,
            };
            Ok(Telling {
                sections: vec![(
                    "environment".into(),
                    environment(place, &places, mode, octoii::machine::confines(), volume.now_ms()?),
                )],
                tools: Vec::new(),
            })
        })
    }
}

struct Places<'a> {
    scratchpad: Option<PathBuf>,
    temporary: &'a [PathBuf],
    writable: &'a [Writable],
}

fn environment(place: Option<&Place>, places: &Places<'_>, mode: Mode, confines: bool, now_ms: i64) -> String {
    let mut out = String::from("<environment>\n");
    if let Some(place) = place {
        let _ = writeln!(out, "workspace: {}", place.workspace.display());
    }
    let _ = writeln!(out, "platform: {} {}", std::env::consts::OS, std::env::consts::ARCH);
    let _ = writeln!(out, "shell: {}", if cfg!(windows) { "powershell" } else { "bash" });
    if let Some(scratchpad) = &places.scratchpad {
        let _ = writeln!(
            out,
            "scratchpad: {} — temporary files; commands' TMPDIR; cleared by the system",
            scratchpad.display()
        );
    }
    if !places.temporary.is_empty() {
        let folders: Vec<String> = places.temporary.iter().map(|folder| folder.display().to_string()).collect();
        let _ = writeln!(out, "temporary folders: {}", folders.join(", "));
    }
    for writable in places.writable {
        let _ = writeln!(out, "{}: {}", writable.named, writable.folder.display());
    }
    let _ = writeln!(out, "permissions: {}", permissions(mode, confines));
    let (year, month, day) = civil(now_ms.div_euclid(86_400_000));
    let _ = write!(out, "date: {year}-{month:02}-{day:02} (UTC)\n</environment>");
    out
}

/// What a thread's mode lets its calls do, as the model is told it, on a machine that confines commands or not.
fn permissions(mode: Mode, confines: bool) -> &'static str {
    match (mode, confines) {
        (Mode::Default, true) => {
            "default — edits and sandboxed commands may write within the workspace and listed folders. Other edits, escalated commands and <catalog> tools require approval unless already allowed for this project."
        }
        (Mode::Default, false) => {
            "default — edits within the workspace and listed folders proceed. No command sandbox is available; commands, other edits and <catalog> tools require approval unless already allowed for this project."
        }
        (Mode::Auto, _) => {
            "auto — edits within the workspace and listed folders, and read-only commands, proceed. Other calls are reviewed or require approval unless already allowed for this project. A blocked call includes the reason."
        }
        (Mode::FullAccess, _) => "full-access — every call goes ahead as made.",
    }
}

/// The year, month and day of the day `days` after 1970-01-01, as the proleptic Gregorian calendar has it.
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 { month_index + 3 } else { month_index - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::civil;

    #[test]
    fn a_day_is_its_date_in_the_calendar() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(20_768), (2026, 11, 11));
        assert_eq!(civil(11_016), (2000, 2, 29));
        assert_eq!(civil(-1), (1969, 12, 31));
    }
}
