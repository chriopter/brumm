// The narrow C interface to crates/brumm-shared. Qt owns the objects and
// textures; Rust owns the listener and computes cover colors.
#pragma once

#include <cstddef>
#include <cstdint>

extern "C" {
struct BrummListener;
struct BrummLevels {
    double t, live, bass, mid, high, level, beat, since;
    std::int32_t kicks;
};
enum BrummBands { Spectrum = 0, Smoothed = 1, Peaks = 2 };

BrummListener *brumm_listener_new();
void brumm_listener_free(BrummListener *listener);
void brumm_listener_tick(BrummListener *listener, const double *input,
                         std::size_t count, double dt, bool playing);
void brumm_listener_levels(const BrummListener *listener, BrummLevels *out);
const double *brumm_listener_bands(const BrummListener *listener,
                                  BrummBands which, std::size_t *count);
bool brumm_accent(const std::uint8_t *rgba, std::size_t width,
                  std::size_t height, std::size_t stride,
                  std::uint32_t background, std::uint32_t *out);
}
