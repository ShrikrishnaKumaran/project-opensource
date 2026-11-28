//! On-demand offline MBTiles (SQLite) tile reader.
//!
//! Planned contents:
//! - Open local `.mbtiles` archives without loading full datasets into memory.
//! - Read `metadata` (bounds, min/max zoom, format).
//! - Query individual `(z, x, y)` tiles with TMS `tile_row` conversion.
