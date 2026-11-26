//! The starts under Make's "Devices" tile, for the command line too: `rebuild --device <start>`
//! asks for the same request the app's tile fills in. Nothing here changes a phone: each request
//! only prepares, and a step that changes a connected device waits for the person's own
//! confirmation (see `ActionKind::Device`).
//!
//! The app keeps its own copy of these words, since it shows them translated; its test
//! (`device_starters_say_what_the_cli_says`) holds the two together.

/// One start under Devices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Starter {
    /// Its name on the command line (`--device android-app`).
    pub id: &'static str,
    /// What the tile says it is (English).
    pub title: &'static str,
    /// The request it fills in; `[…]` marks what the person fills in.
    pub request: &'static str,
    /// Needs a file to look inside (an app or a firmware image) and Deep mode to open it.
    pub looks_inside: bool,
}

pub const STARTERS: [Starter; 4] = [
    Starter {
        id: "android-app",
        title: "Android app",
        request: "An Android app in Kotlin that [what it does]. Set it up as a Gradle project that builds a debug APK, and say how to install it on a phone with adb.",
        looks_inside: false,
    },
    Starter {
        id: "research",
        title: "Look inside an app or firmware",
        request: "Look inside the chosen Android app or firmware image: unpack it, decompile it (JADX, Apktool, Ghidra or REA when they're installed) and write a report of what it contains: components, permissions and what each is used for, trackers, native libraries, network addresses and anything risky.",
        looks_inside: true,
    },
    Starter {
        id: "root-flash",
        title: "Root and flash plan",
        request: "A plan to root and flash my phone, model [model]. Write the steps for this exact model: unlocking the bootloader, a custom recovery and root with Magisk, with adb and fastboot scripts that check the device, the battery and the files first and ask me to type a confirmation before each step. Only prepare them: don't run anything that changes the phone.",
        looks_inside: false,
    },
    Starter {
        id: "utilities",
        title: "Phone utilities over adb",
        request: "Tools for my Android phone over adb: back up photos and files to this computer, move data between phones, and automate [task]. Reading steps first; anything that changes the phone only after I confirm.",
        looks_inside: false,
    },
];

/// The start called `id` (as typed after `--device`).
pub fn starter(id: &str) -> Option<&'static Starter> {
    STARTERS.iter().find(|s| s.id.eq_ignore_ascii_case(id.trim()))
}

/// The ids, for a list of what can be typed.
pub fn ids() -> Vec<&'static str> {
    STARTERS.iter().map(|s| s.id).collect()
}

impl Starter {
    /// The request with the person's own words in it: they replace the first `[…]`; a start with
    /// none (looking inside a file) gets them added after. Nothing typed leaves the request as the
    /// tile fills it in.
    pub fn request_with(&self, details: &str) -> String {
        let details = details.trim();
        if details.is_empty() || details == "-" {
            return self.request.to_string();
        }
        match (self.request.find('['), self.request.find(']')) {
            (Some(open), Some(close)) if open < close => format!("{}{details}{}", &self.request[..open], &self.request[close + 1..]),
            _ => format!("{} {details}", self.request),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_typed_fills_the_first_blank_or_is_added_after() {
        let app = starter("android-app").unwrap();
        assert!(app.request_with("shows battery health").contains("Kotlin that shows battery health. Set it up"));
        assert!(!app.request_with("x").contains('['));
        assert_eq!(app.request_with("  "), app.request);
        assert_eq!(app.request_with("-"), app.request);
        let look = starter("RESEARCH").unwrap();
        assert!(look.request_with("focus on trackers").ends_with(" focus on trackers"));
        assert!(look.looks_inside && !app.looks_inside);
        assert!(starter("flash-everything").is_none());
    }

    #[test]
    fn every_start_only_prepares_what_changes_a_phone() {
        let ids = ids();
        assert_eq!(ids, ["android-app", "research", "root-flash", "utilities"]);
        let plan = starter("root-flash").unwrap().request;
        assert!(plan.contains("Only prepare them") && plan.contains("confirmation"), "{plan}");
        assert!(starter("utilities").unwrap().request.contains("only after I confirm"));
    }
}
