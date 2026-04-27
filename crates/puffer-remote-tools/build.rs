fn main() {
    let protoc = protoc_bin_vendored::protoc_bin_path().expect("protoc path");
    std::env::set_var("PROTOC", protoc);
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&["proto/tool_runner.proto"], &["proto"])
        .expect("compile tool runner proto");
}
