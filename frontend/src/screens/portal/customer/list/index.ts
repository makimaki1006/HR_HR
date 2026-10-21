import { createElement } from 'react';
import type { PreviewScreen } from '../../preview';
import { ApplicantList } from './ApplicantList';

export const screens: PreviewScreen[] = [
  { id: 'list', group: '顧客（ログイン後）', title: '応募者の一覧', render: () => createElement(ApplicantList) },
  { id: 'list-empty', group: '顧客（ログイン後）', title: '一覧（応募者がまだいない）', render: () => createElement(ApplicantList, { forceState: 'empty' }) },
  { id: 'list-unavailable', group: '顧客（ログイン後）', title: '一覧（表示できない）', render: () => createElement(ApplicantList, { forceState: 'unavailable' }) },
  { id: 'list-stale', group: '顧客（ログイン後）', title: '一覧（少し前の内容）', render: () => createElement(ApplicantList, { forceState: 'stale' }) },
];
