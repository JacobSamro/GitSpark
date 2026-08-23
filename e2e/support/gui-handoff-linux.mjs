// The Linux GUI-handoff image (docker/e2e-linux-desktop) registers the
// stub app as the real default handler for everything this suite needs at
// build time (see its Dockerfile), so there's no per-run registration to
// do here — this module just resolves where the stub writes its marker.
export function markerPath() {
  return process.env.GITSPARK_GUI_MARKER_FILE || "/tmp/gitspark-gui-marker.log";
}

export async function setupStub() {
  // Registration already happened at image build time.
}

export async function teardownStub() {
  // Nothing to undo — the container is thrown away after the run.
}
