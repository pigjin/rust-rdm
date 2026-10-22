fn main() {
    let mut args = std::env::args_os().skip(1);
    let data_dir = args.next().expect("usage: render_qa DATA_DIR OUTPUT_DIR");
    let output_dir = args.next().expect("usage: render_qa DATA_DIR OUTPUT_DIR");
    redisdock::ui::run_qa(data_dir.into(), output_dir.into());
}
