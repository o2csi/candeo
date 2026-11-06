// TypeScript 5.9.3 (Monaco 0.57.0), transpileModule for ES2020, before #381. Source SHA-256: 77c907d1b2c767669e2d93bd17777e82e324af840d5077456b827ca92538d218
import { defineEffect } from '@candeo/effects-api';
// A field initialised from a parameter property: common, and the one place the
// ES2020 target changes what runs. Compiled for ES2020, TypeScript assigns the
// parameter property first and the field after (`useDefineForClassFields`
// false), so `level` is 2 × 50. A native class field runs before the
// constructor body, where `this.base` is still undefined.
class Pulse {
    constructor(base) {
        this.base = base;
        this.level = this.base * 2;
    }
}
export default defineEffect({
    render({ frame }) {
        const p = new Pulse(50);
        frame.fill({ r: Number.isNaN(p.level) ? 255 : p.level, g: 0, b: Number.isNaN(p.level) ? 255 : 0 });
    },
});
