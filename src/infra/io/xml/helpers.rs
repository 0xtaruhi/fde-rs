use roxmltree::Node;

pub(super) fn attr(node: &Node<'_, '_>, name: &str) -> String {
    node.attribute(name).unwrap_or_default().to_string()
}
