function plural(count, singular, pluralForm = `${singular}s`) {
  return `${count} ${count === 1 ? singular : pluralForm}`;
}

export function bulkShareMessage({ email, shared = [], already_shared = [], skipped = [] }) {
  const parts = [];
  if (shared.length) {
    parts.push(`Shared ${plural(shared.length, 'project')} with ${email}.`);
  } else {
    parts.push('No new projects shared.');
  }
  if (already_shared.length) {
    parts.push(`${already_shared.length} ${already_shared.length === 1 ? 'was' : 'were'} already shared.`);
  }
  if (skipped.length) {
    parts.push(`${plural(skipped.length, 'skipped')}.`);
  }
  return parts.join(' ');
}
