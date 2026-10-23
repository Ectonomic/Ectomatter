//! Every sensor on a Linux computer, for the Sensor Feed plugin (plugins/sensor-feed),
//! which lists and reads them, and for Ectomatter, which shows what the plugin
//! tells it: what a sensor is ([list]), and how a reading is written ([read]).
//! Finding and reading the sensors is behind the `read` feature, so
//! Ectomatter, which builds without it, can't read them itself.

#![cfg_attr(not(feature = "read"), allow(dead_code))]

pub mod files;
pub mod list;
pub mod parts;
pub mod read;
#[cfg(any(test, feature = "fake"))]
pub mod fake;
