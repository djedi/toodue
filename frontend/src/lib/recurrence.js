export const repeatOptions = [
  { value: '', label: 'Does not repeat' },
  { value: 'daily', label: 'Every day' },
  { value: 'weekly', label: 'Every week' },
  { value: 'monthly', label: 'Every month' },
  { value: 'yearly', label: 'Every year' }
];

export function repeatLabel(rule) {
  return repeatOptions.find((option) => option.value === (rule ?? ''))?.label ?? 'Does not repeat';
}

export function normalizeRepeatRule(rule, dueDate) {
  return dueDate && rule ? rule : null;
}
