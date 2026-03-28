import { BreakpointString, Status } from './types';

// Mapped type: all BreakpointString members are used as keys
type BreakpointValues = { [K in BreakpointString]?: number };

// Qualified name in type position: Status.Active is used
type ActiveOnly = Status.Active;

const breakpoints: BreakpointValues = {
    xs: 0,
    sm: 576,
};

// Runtime access — only Status.Inactive directly accessed
const s = Status.Inactive;

console.log(breakpoints, s);
