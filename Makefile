.PHONY: test

test:
	xvfb-run --auto-servernum --server-args="-screen 0 1280x1024x24 -extension GLX" cargo test -- --test-threads=1
