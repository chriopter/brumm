// The scene covers the screen; its main subject fits inside activeArea.
vec2 areaUv(vec2 screen) {
    return (screen - activeArea.xy) / max(activeArea.zw, vec2(0.001));
}
float areaFocus(vec2 uv) {
    vec2 edge = min(uv, 1.0 - uv);
    return smoothstep(0.0, 0.065, edge.x) * smoothstep(0.0, 0.065, edge.y);
}
vec2 areaCenter() { return activeArea.xy + activeArea.zw * 0.5; }
