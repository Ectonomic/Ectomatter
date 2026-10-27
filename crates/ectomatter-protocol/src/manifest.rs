//! A package's manifest, `core.json` (docs/core-protocol.md, section 3, "A
//! core on disk", and section 11): every field it may give, typed. A field
//! the protocol doesn't name is an error, so a misspelled one is found as the
//! package is read rather than passed over.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A package's manifest.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Manifest {
    pub id: String,
    /// which the package is; a manifest without one is a core if it names the
    /// devices it reaches (`matches`), a plugin otherwise (`Manifest::kind`)
    #[serde(default, rename = "kind", skip_serializing_if = "Option::is_none")]
    pub kind_given: Option<Kind>,
    pub name: String,
    pub version: String,
    /// the core protocol it speaks
    pub protocol: u32,
    /// the oldest Ectomatter it runs on
    pub ectomatter: String,
    /// its code, built for wasm32-wasip2, a file in its folder
    pub module: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// a sentence of what it's for, in its own words
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// an SVG file in its folder for its tile
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glyph: Option<String>,
    /// where its releases are on GitHub, "owner/name"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    /// a core's device's description, a file in its folder (section 4)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_file: Option<String>,
    /// the devices it works with, by name (none named: its own name's; an
    /// empty list: none of its own, as a plugin bringing in another
    /// program's has)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub devices: Option<Vec<String>>,
    /// the whole of what it may open
    #[serde(default)]
    pub matches: Vec<Match>,
    /// what it does for itself beyond its devices
    #[serde(default)]
    pub uses: Vec<Use>,
    /// what it adds to the frontend's pages
    #[serde(default)]
    pub adds: Vec<Adds>,
    /// the systems it works on, as `std::env::consts::OS` names them; every
    /// one where it names none
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub systems: Option<Vec<String>>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Core,
    Plugin,
}

/// A device a core may open: by its USB ids, and where a device shows
/// several interfaces, the one it talks to.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Match {
    pub usb: Usb,
}

/// USB ids in hex, as the system lists them ("3842").
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Usb {
    pub vendor: String,
    pub product: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interface: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_page: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<String>,
}

/// One thing a package does for itself.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(untagged, deny_unknown_fields)]
pub enum Use {
    /// `"monitors"`, `"nvidia"`, `"launch"`, `"sound-capture"`, `"network"`
    Word(Word),
    /// an address it may connect to, and no other
    Network {
        network: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to: Option<String>,
    },
    /// a web host it may ask, and no other
    Web {
        web: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to: Option<String>,
    },
    /// a file or folder it may read, whole, from /
    Read { read: String },
    /// how its games start: the programs they may run through, the
    /// variables they may set, the programs its buttons may run (section 11,
    /// A game's start; read by the frontend's launcher)
    Launch { launch: Value },
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Word {
    Monitors,
    Nvidia,
    Launch,
    SoundCapture,
    Network,
}

/// What a plugin adds to the frontend's pages.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Adds {
    /// a section on each game's and app's page
    Game,
    /// a section of its own in Settings
    Settings,
    /// readings, for the screens that show them
    Readings,
}

impl Manifest {
    /// A manifest from its JSON (its comments already out): every field
    /// the protocol names, and none it doesn't.
    pub fn of(v: &Value) -> Result<Manifest, String> {
        Manifest::deserialize(v).map_err(|e| e.to_string())
    }

    /// Which the package is, given or as its manifest reads.
    pub fn kind(&self) -> Kind {
        self.kind_given.unwrap_or(if self.matches.is_empty() { Kind::Plugin } else { Kind::Core })
    }

    pub fn adds(&self, a: Adds) -> bool {
        self.adds.contains(&a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn minimal() -> Value {
        json!({ "id": "x", "name": "X", "version": "1.0.0", "protocol": 4, "ectomatter": "0.1.0", "module": "core.wasm" })
    }

    #[test]
    fn a_manifest_reads_with_only_what_it_must_give() {
        let m = Manifest::of(&minimal()).unwrap();
        assert_eq!(m.kind(), Kind::Plugin, "no matches: a plugin");
        assert!(m.uses.is_empty() && m.adds.is_empty() && m.systems.is_none());
    }

    #[test]
    fn a_field_the_protocol_doesnt_name_is_an_error() {
        let mut v = minimal();
        v["deviceFiel"] = json!("device.json");
        assert!(Manifest::of(&v).unwrap_err().contains("deviceFiel"));
        let mut v = minimal();
        v["matches"] = json!([{ "usb": { "vendor": "3842", "product": "2611", "usage-page": "0008" } }]);
        assert!(Manifest::of(&v).is_err(), "a misspelled field in a match");
        let mut v = minimal();
        v["uses"] = json!(["monitor"]);
        assert!(Manifest::of(&v).is_err(), "a use the protocol doesn't name");
        let mut v = minimal();
        v["uses"] = json!([{ "web": "api.open-meteo.com", "too": "Open-Meteo" }]);
        assert!(Manifest::of(&v).is_err(), "a misspelled field in a use");
    }

    #[test]
    fn every_kind_of_use_reads() {
        let mut v = minimal();
        v["uses"] = json!(["monitors", "nvidia", "sound-capture", { "network": "127.0.0.1:6742", "to": "OpenRGB" }, { "web": "api.open-meteo.com", "to": "Open-Meteo" }, { "read": "/sys" }, { "launch": { "tools": ["winecfg"] } }]);
        v["adds"] = json!(["game", "settings", "readings"]);
        let m = Manifest::of(&v).unwrap();
        assert_eq!(m.uses.len(), 7);
        assert!(matches!(&m.uses[3], Use::Network { network, .. } if network == "127.0.0.1:6742"));
        assert!(m.adds(Adds::Settings));
    }
}
