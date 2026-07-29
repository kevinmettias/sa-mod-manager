/// Whether a file name is a streamed asset ModLoader resolves by resource name
/// (so its subfolder is irrelevant to conflicts).
fn is_streamed_by_name(name: &str) -> bool
{
    return [".dff", ".txd", ".col", ".ifp"]
        .iter()
        .any(|ext| name.ends_with(ext));
}
