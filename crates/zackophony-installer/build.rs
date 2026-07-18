fn main() {
	let mut res = winres::WindowsResource::new();
	res.set_icon("assets/images/icon.ico");
	res.set_manifest_file("./app.manifest");
	res.compile().unwrap();
}
