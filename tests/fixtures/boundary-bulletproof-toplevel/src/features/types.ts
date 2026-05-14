// Top-level non-barrel file. Deliberately imports from `src/app/` to exercise
// the trade-off: under the Bulletproof preset, top-level files inside
// `src/features/` are unclassified, so this import is unrestricted and does
// NOT produce a boundary violation. A future change that re-introduces a
// `patterns: ["src/features/**"]` fallback on the preset would silently
// classify this file under `features` and flip the assertion in
// `bulletproof_top_level_features_file_is_unrestricted`.
import { page } from '../app/page';
export const Toplevel = page;
