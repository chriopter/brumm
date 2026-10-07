package launch

import "testing"

func TestPreviewSourceOnlySelectsEligiblePrototype(t *testing.T) {
	normal := previewWindow{Address: "old", Class: "brumm", Title: "brumm · API prototype · sample data", Mapped: true}
	normal.Workspace.ID = 4
	unrelated := normal
	unrelated.Title = "Real music · brumm"
	floating := normal
	floating.Floating = true
	grouped := normal
	grouped.Grouped = []string{"other"}
	hidden := normal
	hidden.Workspace.ID = -99
	for _, w := range []previewWindow{unrelated, floating, grouped, hidden} {
		if previewSource([]previewWindow{w}, TUI) != nil {
			t.Fatalf("ineligible window chosen: %+v", w)
		}
	}
	if got := previewSource([]previewWindow{unrelated, normal}, TUI); got == nil || got.Address != "old" {
		t.Fatal("prototype not selected")
	}
	if previewSource([]previewWindow{normal}, GUI) != nil {
		t.Fatal("selected same face as replacement")
	}
}
