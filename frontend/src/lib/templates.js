export const packingListTemplate = {
  id: 'builtin:packing-list',
  slug: 'packing-list',
  name: 'Packing List',
  description: 'Reusable travel packing checklist',
  color: 'sky',
  is_builtin: true,
  tasks: [
    { name: 'Wallet / ID' },
    { name: 'Keys' },
    { name: 'Phone charger' },
    { name: 'Laptop / tablet charger' },
    { name: 'Medications' },
    { name: 'Toiletries' },
    { name: 'Toothbrush and toothpaste' },
    { name: 'Underwear' },
    { name: 'Socks' },
    { name: 'Shirts' },
    { name: 'Pants / shorts' },
    { name: 'Sleepwear' },
    { name: 'Shoes' },
    { name: 'Jacket / hoodie' },
    { name: 'Laundry bag' },
    { name: 'Snacks / water bottle' }
  ]
};

export function templatePreviewCount(template) {
  return template.task_count ?? template.tasks?.length ?? 0;
}
