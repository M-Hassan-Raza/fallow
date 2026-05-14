// Top-level barrel: re-exports children. The Bulletproof preset must NOT
// classify this file under the `features` zone, otherwise re-exporting
// children would produce `features -> features/<child>` false positives.
import { authPage } from './auth/login';
import { Toplevel } from './types';
export const features = authPage + Toplevel;
