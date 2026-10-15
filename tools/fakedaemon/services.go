package main

import "encoding/json"

// The preview uses the same shape as the real daemon, without credentials.
func serviceInfo(selected string, configured map[string]bool, generation int64) json.RawMessage {
	fixture := providerFixture
	if selected == "tidal" {
		fixture = tidalProviderFixture
	}
	var info map[string]any
	_ = json.Unmarshal(fixture, &info)
	info["selected"] = selected
	info["generation"] = generation
	info["needsChoice"] = selected == ""
	info["needsSetup"] = !configured["apple"] && !configured["tidal"]
	info["choices"] = []map[string]any{
		{"id": "apple", "name": "Apple Music", "available": true, "configured": configured["apple"]},
		{"id": "tidal", "name": "Tidal", "available": true, "configured": configured["tidal"]},
	}
	if selected == "" {
		entries := info["navigation"].([]any)
		info["navigation"] = entries[len(entries)-2:]
		info["capabilities"] = map[string]any{}
	}
	encoded, _ := json.Marshal(info)
	return encoded
}
