//! Offline campus map UI component (Dioxus).
//!
//! Planned contents:
//! - Render offline MBTiles and GeoJSON boundary/building/road overlays.
//! - Pass local device coordinates to `domain::geo::GeofenceEngine` and display
//!   the resolved `DetectedLocation` without logging or transmitting raw GPS.
