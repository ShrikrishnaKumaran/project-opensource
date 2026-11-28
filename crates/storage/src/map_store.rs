//! Offline GeoJSON campus map loader and validator.
//!
//! Planned contents:
//! - Load campus boundaries, buildings, roads, and predefined locations from
//!   an external directory (replaceable without rebuilding the application).
//! - Validate RFC 7946 `[lon, lat]` coordinates, closed polygon rings, and
//!   required metadata (`id`, `name`), handling malformed/unsupported features
//!   gracefully.
