# Microcount (Rust)

> This port was generated with the assistance of [Claude](https://claude.com/claude-code)
> (Anthropic's AI coding assistant), working from the original MATLAB source, and was
> validated against the MATLAB version on real data (see below).

A native port of the [MATLAB Microcount app](https://github.com/AlbertUgwudike/microcount-app): semi-automated segmentation and morphometry
of microglia and astrocytes in brain sections registered to the Allen CCF atlas. It has the
same workflow and tabs (Home → Select Images → Register → Select Regions → Analyse), the
same six algorithms, and the same output files.

## Building and running

```bash
cargo run --release
```

Requirements:

- A Rust toolchain (1.80+).
- The Allen CCF atlas files in `assets/` (`annotation.tiff`, `reference.tiff`, `structures.csv`), included in the repository.
- Java, to import non-TIFF formats (`.lif`, `.lof`, `.czi`, …) through the bundled Bio-Formats converter `convert/convert.jar`. TIFF files are read directly and don't need Java.

The app looks for `assets/` and `convert/convert.jar` next to the executable, in a macOS
bundle's `Resources`, in the working directory and its parent, and in the repository root. Set
`MICROCOUNT_HOME` to the folder that contains them when installing elsewhere.

### Headless use

`microcount-cli` runs any algorithm on a TIFF (or part of one). It's useful for batch jobs
and for comparing against MATLAB:

```bash
cargo run --release --bin microcount-cli -- run image.tiff --algo micro-fluor --cell 2 --co 3 --px 0.2876,0.2876 --rect 10000,8000,3000,3000 --out results/
```

`examples/e2e.rs` runs the whole workflow (workspace → import → registration → region →
processing → CSV) without the GUI.

## Layout

| Path | Contents |
| --- | --- |
| `src/core/tiff.rs` | (Big)TIFF reader that decodes only the strips/tiles under a requested region (LZW, Deflate, PackBits, predictors). Also a streaming writer. |
| `src/core/imgproc/` | Equivalents of the Image Processing Toolbox functions used: `log_norm`, `mat2gray`, `stretchlim`/`imadjust`, `pacefilt`, disk morphology, labelling, `imfill`, `bwperim`, `bwskel`, CLAHE, `wiener2`, `medfilt2`, convex hulls. |
| `src/core/analysis/` | The six algorithms (`algorithms.rs`), the shared cell pipeline — flood fill, convexity, branches, branch lengths, Scholl (`cells.rs`) — `data2result` (`data.rs`) and output images (`render.rs`). |
| `src/core/atlas.rs`, `registration.rs` | Atlas slicing and warping, region masks, control-point and automatic registration. |
| `src/core/workspace.rs`, `pipeline.rs`, `convert.rs` | Workspace model, per-region processing, preview and export, image import. |
| `src/gui/` | egui front end. Long work runs on background threads that report back over a channel. |

## Validation against MATLAB

Each algorithm was run in both versions on identical crops of real data with MATLAB R2026b.

| Algorithm | Agreement |
| --- | --- |
| Fluorescent microglia | Soma, co-marker and foreground masks, cell count, density and soma size identical. |
| DAB microglia | Soma and foreground masks, cell count and density identical. |
| DAB astrocytes | Same cell count; masks within 0.07%. |
| Fluorescent astrocytes | Masks within 0.05%; 18 vs 17 cells. |
| Plaques | Consistent: neither version found plaques in the test crop. |
| Single astrocyte | Identical. |

The flood-fill cell labelling matches MATLAB pixel for pixel, and so does branch-point
detection on the same skeleton.

The skeleton (`bwskel`) is the one known approximation. It is Lee's thinning (the
algorithm MATLAB uses), but MATLAB's built-in visits pixels in an order that couldn't be
reproduced exactly. Skeleton size agrees to about 2%, and tip counts and branch lengths to
a few percent. Branch-derived metrics (branch points, branch lengths, Scholl index) can
therefore differ slightly from MATLAB's.

Speed: 10–40× faster than MATLAB on the same crops. The full workflow on a
24779×18457 section runs in under 2 s (import, registration, processing a 19-megapixel
region, export), peaking below 1 GB of RAM.

## Deliberate differences from the MATLAB version

These are bug fixes or improvements. Results that depend on them will differ from MATLAB's.

- **Cell area %** counts each foreground pixel once. MATLAB summed a `uint16` mask holding
  2 where soma and process overlapped, so it double-counted those pixels and inflated
  *Cell Area %* (sometimes past 100%). It also deflated *CoMarker % Area*, which divides by that area.
- **Plaque split**: the in-plaque and out-of-plaque results use the cell and activation
  counts of their own subset. MATLAB used the total cell count for both. The out-of-plaque
  result is now also exported to the CSV as `<region>_out_plaque`.
- **Average branch point count** is no longer rounded to an integer (it was a `uint16`).
- **Sagittal hemisphere split** uses the sagittal midline, matching the laterality label.
  MATLAB used the row count there.
- **Region masks** are upscaled from the downsampled mask with interpolation, never
  materialising a full-resolution mask of the whole image. Processing reads only the
  region's bounding box.
- **Previews** cover the drawn rectangle intersected with the region, using the
  normalisation of the whole region (cached while you adjust thresholds).
- **Downsampled images** keep every channel, so any channel can be used for registration.
- **Automatic registration** starts by fitting the atlas brain to the tissue's bounding
  box, then refines a mutual-information fit on smoothed silhouettes. It replaces
  `imregtform` and gives a much closer starting point for the control-point step.
- **Rotating an image or changing atlas orientation** resets its alignment instead of
  leaving a transform that no longer fits.
- **Workspaces** are stored as `ws.json`, with the same `ws_converted`, `ws_downsampled`,
  `ws_processed`, `ws_masks` and `ws_scholl` folders. MATLAB `ws.mat` workspaces can't be
  opened: create a new workspace and re-add the images (already-converted TIFFs are
  imported without re-conversion).
- **Interrupted work**: conversions or processing that were running when the app closed
  are reset on load instead of staying stuck as "converting"/"processing".
- **Output images** are written as BigTIFF only when they need to be. Soma outlines are
  saved at full intensity (MATLAB wrote 1).
- **Removed**: the user-specific "Magic" button and the unfinished NeuN algorithm, which
  could not run.
