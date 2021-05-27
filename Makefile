all:
	cargo build
	cp -f ./target/debug/libcut_ts.so /opt/devtek/bin/
