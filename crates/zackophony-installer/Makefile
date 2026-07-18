.PHONY: all build

build:
	rm Zackophony -rf
	mkdir -p Zackophony/App
	cd ../zackophony && cargo build --release
	mv ../../target/release/zackophony.exe Zackophony/App/Zackophony.exe
	cp -r Resources Zackophony
	cargo build --release
	cp ../../target/release/zackophony-installer.exe "Zackophony/Zackophony Installer.exe"
