# ADR 0001: Offline Campus Map and Privacy-Preserving Geofencing

- **Status:** Proposed
- **Date:** 2026-11-27

## Context

The application needs an offline-first campus map and geofence location detection feature (intended to power a future friends-location feature) that works on both Windows Desktop and Android without relying on online tile servers or a separate backend.

Key constraints and requirements:
1. **Offline-first & replaceable data:** Campus boundaries, buildings, roads, and predefined zones are stored locally as GeoJSON, and map tiles are stored locally as MBTiles. Map data must be replaceable or updatable on disk without rebuilding the application.
2. **Privacy:** Raw GPS coordinates (`latitude`, `longitude`) must remain strictly on the user's device. They must never be logged, persisted for sync, or transmitted to any realtime/friends layer—only the resolved `LocationId` may cross into `sync`.
3. **Fit existing architecture:** The workspace already follows a strict layered crate layout (`domain` -> `storage` -> `sync` -> `app`) with `unsafe_code = "forbid"` and `clippy::pedantic` enabled.
4. **Lightweight for mobile:** Avoid heavy geospatial dependencies and avoid loading full tile archives into memory.

## Decision

### 1. Layer Placement (No New Crates)

| Crate | File / Module | Responsibility |
|---|---|---|
| `domain` | `crates/domain/src/geo.rs` | Pure geospatial types (`GeoPoint`, `LocationId`, `LocationCategory`, `CampusLocation`, `DetectedLocation`, `LocationMatch`, `MapTile`, `TileFormat`, `GeoError`) and the zero-allocation `GeofenceEngine` (AABB prefilter + point-in-polygon with holes, boundary inclusion, overlap priority resolution). Zero I/O and zero external dependencies. |
| `storage` | `crates/storage/src/map_store.rs` | External GeoJSON loading and validation (enforcing `[lon, lat]` coordinate order, ring closure, valid bounds, and graceful handling of malformed/unsupported features). |
| `storage` | `crates/storage/src/mbtiles.rs` | On-demand SQLite MBTiles reader (`metadata` + `(z, x, y)` tile lookup with TMS Y-flip) so tiles are streamed individually rather than loaded into RAM. |
| `sync` | `crates/sync/src/lib.rs` | Privacy boundary: accepts/exposes only `DetectedLocation` / `LocationId`, never raw `GeoPoint` coordinates. |
| `app` | `crates/app/src/map_view.rs` | Dioxus UI component for rendering offline tiles and GeoJSON overlays and invoking local geofence detection. |

### 2. Dependency Choices

- **`geojson` (`0.24`) in `crates/storage`:** Reuses `serde_json` (already in the workspace) for standards-compliant GeoJSON parsing (`FeatureCollection`, `Polygon`, `MultiPolygon`, `LineString`, `Point`).
- **`rusqlite` (`0.32`, `features = ["bundled"]`) in `crates/storage`:** Reads `.mbtiles` archives without requiring a system `sqlite3` library on Windows (`x86_64-pc-windows-gnu`) or Android (`aarch64-linux-android`).
- **`crates/domain` remains dependency-free:** Ray-casting / winding-number point-in-polygon and bounding-box checks are implemented directly in safe Rust, keeping `domain` fast to compile and allocation-free during lookups.

### 3. Coordinate & Overlap Conventions

- **GeoJSON:** Always parsed as `(longitude, latitude)` per RFC 7946 and converted to `(lat, lon)` only inside validated `GeoPoint` / `GeofenceEngine` inputs.
- **MBTiles:** Standard XYZ `(z, x, y)` requests are converted to TMS `tile_row = (1 << z) - 1 - y` when querying SQLite.
- **Overlapping zones:** Resolved deterministically by explicit `priority` (e.g. `Building` > `Zone` > `CampusBoundary`), breaking ties by smallest bounding-box area so specific locations always win over enclosing areas. Points on polygon edges or vertices count as inside; points outside all zones return `LocationMatch::OutsideCampus`.

## Consequences

- **Positive:**
  - Clean separation between pure math (`domain`), disk/SQLite I/O (`storage`), privacy-safe state (`sync`), and rendering (`app`).
  - Zero network dependency for maps or location resolution.
  - Map data updates require only swapping local `.geojson` / `.mbtiles` files.
- **Trade-offs:**
  - `rusqlite` with `bundled` compiles SQLite from C during `storage` builds, slightly increasing first-build time on Android/Windows.
  - Spatial indexing uses per-polygon AABBs rather than an R-tree; this is optimal for campus-scale datasets (tens to hundreds of polygons) and avoids extra dependencies, though an R-tree could be swapped in behind `GeofenceEngine` later without changing callers.
