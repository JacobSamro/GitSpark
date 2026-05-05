export function findNode(node, predicate) {
  if (!node) {
    return null;
  }
  if (predicate(node)) {
    return node;
  }
  for (const child of node.children || []) {
    const match = findNode(child, predicate);
    if (match) {
      return match;
    }
  }
  return null;
}

export function nodeById(tree, id) {
  return findNode(tree, (node) => node.id === id);
}

export function nodeByTestId(tree, testId) {
  return findNode(tree, (node) => node.test_id === testId);
}

export function flattenNodes(node) {
  if (!node) {
    return [];
  }
  return [node, ...(node.children || []).flatMap(flattenNodes)];
}
