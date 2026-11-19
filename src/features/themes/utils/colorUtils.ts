/**
 * Color utility functions for theme management
 */

interface HSL {
    h: number;
    s: number;
    l: number;
}

/**
 * Convert hex color to HSL
 */
function hexToHsl(hex: string): HSL {
    // Remove the hash if present
    hex = hex.replace(/^#/, "");

    // Parse the hex values
    const r = parseInt(hex.substr(0, 2), 16) / 255;
    const g = parseInt(hex.substr(2, 2), 16) / 255;
    const b = parseInt(hex.substr(4, 2), 16) / 255;

    const max = Math.max(r, g, b);
    const min = Math.min(r, g, b);
    const diff = max - min;

    let h = 0;
    let s = 0;
    const l = (max + min) / 2;

    if (diff !== 0) {
        s = l > 0.5 ? diff / (2 - max - min) : diff / (max + min);

        switch (max) {
            case r:
                h = ((g - b) / diff + (g < b ? 6 : 0)) / 6;
                break;
            case g:
                h = ((b - r) / diff + 2) / 6;
                break;
            case b:
                h = ((r - g) / diff + 4) / 6;
                break;
        }
    }

    return {
        h: Math.round(h * 360),
        s: Math.round(s * 100),
        l: Math.round(l * 100),
    };
}

/**
 * Convert HSL to hex color
 */
function hslToHex(h: number, s: number, l: number): string {
    h = h / 360;
    s = s / 100;
    l = l / 100;

    const hue2rgb = (p: number, q: number, t: number) => {
        if (t < 0) t += 1;
        if (t > 1) t -= 1;
        if (t < 1 / 6) return p + (q - p) * 6 * t;
        if (t < 1 / 2) return q;
        if (t < 2 / 3) return p + (q - p) * (2 / 3 - t) * 6;
        return p;
    };

    let r: number, g: number, b: number;

    if (s === 0) {
        r = g = b = l;
    } else {
        const q = l < 0.5 ? l * (1 + s) : l + s - l * s;
        const p = 2 * l - q;
        r = hue2rgb(p, q, h + 1 / 3);
        g = hue2rgb(p, q, h);
        b = hue2rgb(p, q, h - 1 / 3);
    }

    const toHex = (c: number) => {
        const hex = Math.round(c * 255).toString(16);
        return hex.length === 1 ? "0" + hex : hex;
    };

    return `#${toHex(r)}${toHex(g)}${toHex(b)}`;
}

/**
 * Generate a complete color shade palette from a base color
 */
export function generateColorShades(baseColor: string): string[] {
    const hsl = hexToHsl(baseColor);

    // Define lightness values for each shade (0-9)
    const lightnessValues = [95, 85, 75, 65, 55, 45, 35, 25, 15, 8];

    return lightnessValues.map((lightness) => {
        // Adjust saturation based on lightness for better visual harmony
        let adjustedSaturation = hsl.s;

        if (lightness > 80) {
            // Reduce saturation for very light colors
            adjustedSaturation = Math.max(hsl.s * 0.3, 10);
        } else if (lightness < 20) {
            // Slightly reduce saturation for very dark colors
            adjustedSaturation = Math.max(hsl.s * 0.8, 20);
        }

        return hslToHex(hsl.h, adjustedSaturation, lightness);
    });
}
