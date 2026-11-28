//! Pure geospatial domain types and offline geofence detection engine.
//!
//! Zero I/O and zero external dependencies.
//!
//! Planned contents:
//! - Coordinate & identifier types (`GeoPoint`, `LocationId`, `LocationCategory`)
//! - Campus feature & tile types (`CampusLocation`, `DetectedLocation`, `LocationMatch`, `MapTile`, `TileFormat`, `GeoError`)
//! - `GeofenceEngine`: AABB prefilter + point-in-polygon (exterior + hole rings),
//!   boundary edge/vertex inclusion, and deterministic overlap resolution.
