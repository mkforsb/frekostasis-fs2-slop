# Convenience targets. Requires the Dioxus CLI (`dx`, 0.7.x) and the
# wasm32-unknown-unknown target (`rustup target add wasm32-unknown-unknown`).

APP := crates/app

.PHONY: web desktop serve-web run-desktop test lint renders analyze bench clean

## Serve the web build on http://127.0.0.1:8080
serve-web:
	cd $(APP) && dx serve --web --release

## Run the native Linux app (PulseAudio / PipeWire-pulse)
run-desktop:
	cd $(APP) && dx serve --desktop --release

## Release bundles: target/dx/frekostasis/release/{web/public,linux/app}
web:
	cd $(APP) && dx build --web --release

desktop:
	cd $(APP) && dx build --desktop --release

test:
	cargo test --release -p frekussion-dsp -p frekostasis-dsp -p frekostasis

lint:
	cargo fmt --all --check
	cargo clippy --all-targets -p frekussion-dsp -p frekostasis-dsp -p frekostasis-worklet -- -D warnings
	cargo clippy --all-targets -p frekostasis --features desktop -- -D warnings
	cargo clippy -p frekostasis --features web --target wasm32-unknown-unknown -- -D warnings

## Render every scene (60 s of drone + 10 s of tail) to WAV files in ./renders
renders:
	cargo run --release -p frekostasis-dsp --example render -- renders 60

## Spectral summary of the renders (20-30 s in)
analyze:
	cargo run --release -p frekostasis-dsp --example analyze -- 20 30 renders/*.wav

## Whole-machine speed as a real-time factor, and the slowest block
bench:
	cargo run --release -p frekostasis-dsp --example bench

clean:
	cargo clean
