/** Presentation model for the reference prototype; live API contracts will be generated from Rust. */
export type ObjectType = 'contacts' | 'companies' | 'deals';
export type ActivityType = 'call' | 'note' | 'task' | 'meeting' | 'email';
export type ActivityFilter = ActivityType | 'all';

export interface RecordProperty {
  name: string;
  label: string;
  value: string | null;
}

export interface Activity {
  id: string;
  type: ActivityType;
  title: string;
  body: string;
  occurredAt: string;
  owner: string;
  outcome: string | null;
}

export interface Association {
  objectType: ObjectType;
  id: string;
  label: string | null;
}

export interface CrmRecord {
  id: string;
  objectType: ObjectType;
  name: string;
  subtitle: string;
  owner: string;
  status: string;
  updatedAt: string;
  properties: RecordProperty[];
  activities: Activity[];
  associations: Association[];
  /** Only server-generated links for real IDs. Sample records always use null. */
  deepLink: string | null;
}

export const OBJECT_LABELS: Record<ObjectType, string> = {
  contacts: 'コンタクト', companies: '会社', deals: '取引',
};
export const ACTIVITY_LABELS: Record<ActivityFilter, string> = {
  all: 'すべての活動', call: 'コール', note: 'メモ', task: 'タスク',
  meeting: 'ミーティング', email: 'Eメール',
};

export function findRecords(records: CrmRecord[], objectType: ObjectType, query: string): CrmRecord[] {
  const term = query.trim().toLocaleLowerCase('ja');
  return records.filter(record => record.objectType === objectType &&
    [record.name, record.subtitle, record.owner, ...record.properties.map(p => p.value ?? '')]
      .some(value => value.toLocaleLowerCase('ja').includes(term)));
}

export function filterActivities(
  activities: Activity[], type: ActivityFilter, query: string, owner: string, since: string,
): Activity[] {
  const term = query.trim().toLocaleLowerCase('ja');
  return activities.filter(activity =>
    (type === 'all' || activity.type === type) &&
    (owner === '' || activity.owner === owner) &&
    (since === '' || Date.parse(activity.occurredAt) >= Date.parse(since)) &&
    [activity.title, activity.body, activity.outcome ?? ''].some(value =>
      value.toLocaleLowerCase('ja').includes(term)))
    .sort((a, b) => b.occurredAt.localeCompare(a.occurredAt));
}

export function formatDate(value: string): string {
  return new Intl.DateTimeFormat('ja-JP', {
    timeZone: 'Asia/Tokyo', year: 'numeric', month: '2-digit', day: '2-digit',
    hour: '2-digit', minute: '2-digit',
  }).format(new Date(value));
}

export function associationGroups(record: CrmRecord, records: CrmRecord[]) {
  return (['companies', 'contacts', 'deals'] as ObjectType[]).map(objectType => ({
    objectType,
    records: record.associations.filter(a => a.objectType === objectType).map(association => ({
      association,
      record: records.find(r => r.id === association.id && r.objectType === association.objectType),
    })),
  })).filter(group => group.objectType !== record.objectType || group.records.length > 0);
}
