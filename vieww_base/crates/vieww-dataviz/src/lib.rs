//! The data-visualisation toolkit — D3, on vieww's geometry.
//!
//! The comparison document names D3 as *the* data-driven visualisation
//! capability: "a data-binding toolkit (not a renderer) — it computes
//! positions while Canvas/SVG renders". vieww had finished chart widgets
//! (line, bar, scatter, pie, donut) but not the toolkit underneath one, so a
//! chart the widgets did not anticipate could not be built. This crate is
//! that toolkit; like D3 it renders nothing — it returns numbers, paths and
//! rectangles, and a painter draws them.
//!
//! | D3 module | here |
//! |---|---|
//! | `d3-scale` (linear, log, pow/sqrt, band, point, ordinal; `nice`, `ticks`, `invert`) | [`scale`] |
//! | `d3-scale-chromatic` / `d3-interpolate` (viridis, turbo, diverging, Tableau10) | [`color`] |
//! | `d3-shape` (line/area with linear, step, monotone, cardinal, basis curves; arc, pie, stack) | [`shape`] |
//! | `selection.data().join()` + `transition()` | [`join`] |
//! | `d3-hierarchy` (stratify, sum, treemap, tidy tree, partition) | [`hierarchy`] |
//! | `d3-force` (Barnes–Hut many-body, links, centre, collide, x/y) | [`force`] |
//! | `d3-contour` (marching squares) | [`contour`] |
//! | `d3-delaunay` (triangulation, Voronoi, nearest-site `find`) | [`delaunay`] |
//! | `d3-geo` (equirectangular, Mercator, orthographic, stereographic, azimuthal equal-area, Albers; graticule, great circles) | [`geo`] |
//! | `d3-array` (`bin`, `quantile`, `extent`) | [`bin`] |
//! | `d3-sankey`, `d3-chord` | [`flow`] |
//! | `d3-zoom`, `d3-brush` | [`interact`] |
//! | Vega-Lite's grammar (data + mark + encodings → scales, axes, legend) | [`spec`] |
//! | Manim's `VectorField`/`StreamLines`/`NumberLine`, Matplotlib `quiver`/`streamplot` | [`field`] |

pub mod bin;
pub mod color;
pub mod contour;
pub mod delaunay;
pub mod field;
pub mod flow;
pub mod force;
pub mod geo;
pub mod hierarchy;
pub mod interact;
pub mod join;
pub mod scale;
pub mod shape;
pub mod spec;
