// TypeScript 5.9.3 (Monaco 0.57.0), transpileModule for ES2020, before #381. Source SHA-256: c70449c63a7c2bf49f6d18ef4e84574675eabb0c38c8ad2b501aee968e5daf23
import { defineEffect } from '@candeo/effects-api';
// A field over an inherited accessor (a type error TypeScript reports, TS2610):
// assigned, it calls the setter; defined, it hides it.
class Base {
    constructor() {
        this.seen = 0;
    }
    set level(v) {
        this.seen = v * 2;
    }
    get level() {
        return this.seen;
    }
}
class Derived extends Base {
    constructor() {
        super(...arguments);
        this.level = 100;
    }
}
export default defineEffect({
    render({ frame }) {
        const d = new Derived();
        frame.fill({ r: d.seen, g: d.level, b: 0 });
    },
});
