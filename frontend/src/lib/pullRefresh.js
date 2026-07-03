export function getPullRefreshState({
  startY,
  currentY,
  scrollTop,
  threshold = 72,
  maxDistance = 96,
  resistance = 2
}) {
  if (scrollTop > 0) return { distance: 0, ready: false };

  const rawDistance = Math.max(0, currentY - startY);
  const distance = Math.min(maxDistance, Math.round(rawDistance / resistance));

  return {
    distance,
    ready: distance >= threshold
  };
}
