import type { CrmRecord } from './model';

export interface MocPropertyDefinition {
  name: string; label: string; type: string; fieldType: string;
  options: { label: string; value: string; hidden: boolean }[]; editable: boolean;
}
// Selected definitions only. No credentials or customer record values are bundled.
export const MOC_PROPERTIES_RETRIEVED_AT = '2026-10-01T08:45:22.635007+00:00';
export const MOC_DEAL_PROPERTIES: Record<string, MocPropertyDefinition> = {
  "bpo_10": {
    "name": "bpo_10",
    "label": "不通時チェック",
    "type": "enumeration",
    "fieldType": "radio",
    "options": [
      {
        "label": "6回以上でなかった",
        "value": "6回以上でなかった",
        "hidden": false
      },
      {
        "label": "現在使われておりません",
        "value": "使われておりません",
        "hidden": false
      },
      {
        "label": "FAX音（コール音なし）",
        "value": "FAX音（コール音なし）",
        "hidden": false
      },
      {
        "label": "常時通話中",
        "value": "常時通話中",
        "hidden": false
      },
      {
        "label": "常時留守録（コール音なし）",
        "value": "常時留守録（コール音なし）",
        "hidden": false
      },
      {
        "label": "常時即切電（コール音なし）",
        "value": "即切電（コール音なし）",
        "hidden": false
      },
      {
        "label": "おつなぎできません",
        "value": "おつなぎできません",
        "hidden": false
      },
      {
        "label": "その他 ※詳細記載必須",
        "value": "その他",
        "hidden": false
      }
    ],
    "editable": true
  },
  "bpo_13": {
    "name": "bpo_13",
    "label": "次回架電日",
    "type": "date",
    "fieldType": "date",
    "options": [],
    "editable": true
  },
  "bpo_14": {
    "name": "bpo_14",
    "label": "次回架電時間",
    "type": "enumeration",
    "fieldType": "radio",
    "options": [
      {
        "label": "8:00",
        "value": "8:00",
        "hidden": false
      },
      {
        "label": "8:15",
        "value": "8:15",
        "hidden": false
      },
      {
        "label": "8:30",
        "value": "8:30",
        "hidden": false
      },
      {
        "label": "8:45",
        "value": "8:45",
        "hidden": false
      },
      {
        "label": "9:00",
        "value": "9:00",
        "hidden": false
      },
      {
        "label": "9:15",
        "value": "9:15",
        "hidden": false
      },
      {
        "label": "9:30",
        "value": "9:30",
        "hidden": false
      },
      {
        "label": "9:45",
        "value": "9:45",
        "hidden": false
      },
      {
        "label": "10:00",
        "value": "10:00",
        "hidden": false
      },
      {
        "label": "10:15",
        "value": "10:15",
        "hidden": false
      },
      {
        "label": "10:30",
        "value": "10:30",
        "hidden": false
      },
      {
        "label": "10:45",
        "value": "10:45",
        "hidden": false
      },
      {
        "label": "11:00",
        "value": "11:00",
        "hidden": false
      },
      {
        "label": "11:15",
        "value": "11:15",
        "hidden": false
      },
      {
        "label": "11:30",
        "value": "11:30",
        "hidden": false
      },
      {
        "label": "11:45",
        "value": "11:45",
        "hidden": false
      },
      {
        "label": "12:00",
        "value": "12:00",
        "hidden": false
      },
      {
        "label": "12:15",
        "value": "12:15",
        "hidden": false
      },
      {
        "label": "12:30",
        "value": "12:30",
        "hidden": false
      },
      {
        "label": "12:45",
        "value": "12:45",
        "hidden": false
      },
      {
        "label": "13:00",
        "value": "13:00",
        "hidden": false
      },
      {
        "label": "13:15",
        "value": "13:15",
        "hidden": false
      },
      {
        "label": "13:30",
        "value": "13:30",
        "hidden": false
      },
      {
        "label": "13:45",
        "value": "13:45",
        "hidden": false
      },
      {
        "label": "14:00",
        "value": "14:00",
        "hidden": false
      },
      {
        "label": "14:15",
        "value": "14:15",
        "hidden": false
      },
      {
        "label": "14:30",
        "value": "14:30",
        "hidden": false
      },
      {
        "label": "14:45",
        "value": "14:45",
        "hidden": false
      },
      {
        "label": "15:00",
        "value": "15:00",
        "hidden": false
      },
      {
        "label": "15:15",
        "value": "15:15",
        "hidden": false
      },
      {
        "label": "15:30",
        "value": "15:30",
        "hidden": false
      },
      {
        "label": "15:45",
        "value": "15:45",
        "hidden": false
      },
      {
        "label": "16:00",
        "value": "16:00",
        "hidden": false
      },
      {
        "label": "16:15",
        "value": "16:15",
        "hidden": false
      },
      {
        "label": "16:30",
        "value": "16:30",
        "hidden": false
      },
      {
        "label": "16:45",
        "value": "16:45",
        "hidden": false
      },
      {
        "label": "17:00",
        "value": "17:00",
        "hidden": false
      },
      {
        "label": "17:15",
        "value": "17:15",
        "hidden": false
      },
      {
        "label": "17:30",
        "value": "17:30",
        "hidden": false
      },
      {
        "label": "17:45",
        "value": "17:45",
        "hidden": false
      },
      {
        "label": "18:00",
        "value": "18:00",
        "hidden": false
      },
      {
        "label": "18:15",
        "value": "18:15",
        "hidden": false
      },
      {
        "label": "18:30",
        "value": "18:30",
        "hidden": false
      },
      {
        "label": "18:45",
        "value": "18:45",
        "hidden": false
      },
      {
        "label": "19:00",
        "value": "19:00",
        "hidden": false
      }
    ],
    "editable": true
  },
  "bpo_16": {
    "name": "bpo_16",
    "label": "タスクメモ",
    "type": "string",
    "fieldType": "textarea",
    "options": [],
    "editable": true
  },
  "bpo_18": {
    "name": "bpo_18",
    "label": "掲載元 ※編集不可",
    "type": "string",
    "fieldType": "text",
    "options": [],
    "editable": false
  },
  "bpo_19": {
    "name": "bpo_19",
    "label": "ハロワ更新データ ※編集不可",
    "type": "string",
    "fieldType": "textarea",
    "options": [],
    "editable": false
  },
  "bpo_21": {
    "name": "bpo_21",
    "label": "担当者名（漢字）",
    "type": "string",
    "fieldType": "text",
    "options": [],
    "editable": true
  },
  "bpo_22": {
    "name": "bpo_22",
    "label": "担当者名（よみ）",
    "type": "string",
    "fieldType": "text",
    "options": [],
    "editable": true
  },
  "bpo_23": {
    "name": "bpo_23",
    "label": "商談予定日",
    "type": "date",
    "fieldType": "date",
    "options": [],
    "editable": true
  },
  "bpo_24": {
    "name": "bpo_24",
    "label": "募集職種（現在）",
    "type": "string",
    "fieldType": "text",
    "options": [],
    "editable": true
  },
  "bpo_25": {
    "name": "bpo_25",
    "label": "採用に関する課題感",
    "type": "enumeration",
    "fieldType": "checkbox",
    "options": [
      {
        "label": "応募がない",
        "value": "応募がない",
        "hidden": false
      },
      {
        "label": "定着しない",
        "value": "定着しない",
        "hidden": false
      },
      {
        "label": "現場の高齢化",
        "value": "現場の高齢化",
        "hidden": false
      },
      {
        "label": "採用まで至らない",
        "value": "採用まで至らない",
        "hidden": false
      },
      {
        "label": "時間がない",
        "value": "時間がない",
        "hidden": false
      },
      {
        "label": "課題無し",
        "value": "課題無し",
        "hidden": false
      },
      {
        "label": "未聴取",
        "value": "未聴取",
        "hidden": false
      },
      {
        "label": "その他（備考へ）",
        "value": "その他（備考へ）",
        "hidden": false
      }
    ],
    "editable": true
  },
  "bpo_3": {
    "name": "bpo_3",
    "label": "架電禁止理由",
    "type": "string",
    "fieldType": "text",
    "options": [],
    "editable": true
  },
  "bpo_32": {
    "name": "bpo_32",
    "label": "URL_求人検索 ※編集不可",
    "type": "string",
    "fieldType": "text",
    "options": [],
    "editable": false
  },
  "bpo_33": {
    "name": "bpo_33",
    "label": "商談方法（bpo用）",
    "type": "enumeration",
    "fieldType": "select",
    "options": [
      {
        "label": "zoom",
        "value": "zoom",
        "hidden": false
      },
      {
        "label": "zoom+電話",
        "value": "zoom+電話",
        "hidden": false
      },
      {
        "label": "電話",
        "value": "電話",
        "hidden": false
      },
      {
        "label": "Meet",
        "value": "Meet",
        "hidden": false
      },
      {
        "label": "Teams",
        "value": "Teams",
        "hidden": false
      },
      {
        "label": "訪問",
        "value": "訪問",
        "hidden": false
      },
      {
        "label": "その他",
        "value": "その他",
        "hidden": false
      }
    ],
    "editable": true
  },
  "bpo_34": {
    "name": "bpo_34",
    "label": "募集種別（bpo用）",
    "type": "enumeration",
    "fieldType": "checkbox",
    "options": [
      {
        "label": "正社員",
        "value": "正社員",
        "hidden": false
      },
      {
        "label": "パート",
        "value": "パート",
        "hidden": false
      },
      {
        "label": "契約社員",
        "value": "契約社員",
        "hidden": false
      },
      {
        "label": "未聴取",
        "value": "未聴取",
        "hidden": false
      }
    ],
    "editable": true
  },
  "bpo_4": {
    "name": "bpo_4",
    "label": "ブロック理由",
    "type": "enumeration",
    "fieldType": "radio",
    "options": [
      {
        "label": "リスト被り（架電被り）",
        "value": "リスト被り",
        "hidden": false
      },
      {
        "label": "本部一括（別拠点一括）",
        "value": "本部一括",
        "hidden": false
      },
      {
        "label": "成約企業（SVに報告）",
        "value": "成約企業",
        "hidden": false
      },
      {
        "label": "商談済企業（SVに報告）",
        "value": "商談済企業",
        "hidden": false
      },
      {
        "label": "廃業",
        "value": "廃業",
        "hidden": false
      },
      {
        "label": "リスト不備",
        "value": "リスト不備",
        "hidden": false
      },
      {
        "label": "非電話対応企業",
        "value": "非電話対応企業",
        "hidden": false
      },
      {
        "label": "クレーム懸念案件",
        "value": "クレーム懸念案件",
        "hidden": false
      },
      {
        "label": "対象外企業",
        "value": "対象外企業",
        "hidden": false
      }
    ],
    "editable": true
  },
  "bpo_40": {
    "name": "bpo_40",
    "label": "接触結果",
    "type": "enumeration",
    "fieldType": "select",
    "options": [
      {
        "label": "受付",
        "value": "受付",
        "hidden": false
      },
      {
        "label": "担当者",
        "value": "担当者",
        "hidden": false
      }
    ],
    "editable": true
  },
  "bpo_42": {
    "name": "bpo_42",
    "label": "担当者会話温度感",
    "type": "enumeration",
    "fieldType": "select",
    "options": [
      {
        "label": "高（前向き）",
        "value": "高（前向き）",
        "hidden": false
      },
      {
        "label": "中（検討余地あり）",
        "value": "中（検討余地あり）",
        "hidden": false
      },
      {
        "label": "低（否定的）",
        "value": "低（否定的）",
        "hidden": false
      },
      {
        "label": "聞く耳なし",
        "value": "聞く耳なし",
        "hidden": false
      }
    ],
    "editable": true
  },
  "bpo_45": {
    "name": "bpo_45",
    "label": "次アクション種別",
    "type": "enumeration",
    "fieldType": "select",
    "options": [
      {
        "label": "再架電",
        "value": "再架電",
        "hidden": false
      },
      {
        "label": "メール送付",
        "value": "メール送付",
        "hidden": false
      },
      {
        "label": "資料送付",
        "value": "資料送付",
        "hidden": false
      },
      {
        "label": "日程調整打診",
        "value": "日程調整打診",
        "hidden": false
      }
    ],
    "editable": true
  },
  "bpo_49": {
    "name": "bpo_49",
    "label": "募集人数（アポ用）",
    "type": "string",
    "fieldType": "text",
    "options": [],
    "editable": true
  },
  "bpo_50": {
    "name": "bpo_50",
    "label": "担当者の役職（アポ用）",
    "type": "string",
    "fieldType": "text",
    "options": [],
    "editable": true
  },
  "bpo_8": {
    "name": "bpo_8",
    "label": "補足（性別や営業時間等）",
    "type": "string",
    "fieldType": "textarea",
    "options": [],
    "editable": true
  },
  "bpo__": {
    "name": "bpo__",
    "label": "商談予定時間（bpo用）",
    "type": "enumeration",
    "fieldType": "select",
    "options": [
      {
        "label": "8:00",
        "value": "8:00",
        "hidden": false
      },
      {
        "label": "8:30",
        "value": "8:30",
        "hidden": false
      },
      {
        "label": "9:00",
        "value": "9:00",
        "hidden": false
      },
      {
        "label": "9:30",
        "value": "9:30",
        "hidden": false
      },
      {
        "label": "10:00",
        "value": "10:00",
        "hidden": false
      },
      {
        "label": "10:30",
        "value": "10:30",
        "hidden": false
      },
      {
        "label": "11:00",
        "value": "11:00",
        "hidden": false
      },
      {
        "label": "11:30",
        "value": "11:30",
        "hidden": false
      },
      {
        "label": "12:00",
        "value": "12:00",
        "hidden": false
      },
      {
        "label": "12:30",
        "value": "12:30",
        "hidden": false
      },
      {
        "label": "13:00",
        "value": "13:00",
        "hidden": false
      },
      {
        "label": "13:30",
        "value": "13:30",
        "hidden": false
      },
      {
        "label": "14:00",
        "value": "14:00",
        "hidden": false
      },
      {
        "label": "14:30",
        "value": "14:30",
        "hidden": false
      },
      {
        "label": "15:00",
        "value": "15:00",
        "hidden": false
      },
      {
        "label": "15:30",
        "value": "15:30",
        "hidden": false
      },
      {
        "label": "16:00",
        "value": "16:00",
        "hidden": false
      },
      {
        "label": "16:30",
        "value": "16:30",
        "hidden": false
      },
      {
        "label": "17:00",
        "value": "17:00",
        "hidden": false
      },
      {
        "label": "17:30",
        "value": "17:30",
        "hidden": false
      },
      {
        "label": "18:00",
        "value": "18:00",
        "hidden": false
      },
      {
        "label": "18:30",
        "value": "18:30",
        "hidden": false
      },
      {
        "label": "19:00",
        "value": "19:00",
        "hidden": false
      },
      {
        "label": "19:30",
        "value": "19:30",
        "hidden": false
      },
      {
        "label": "20:00",
        "value": "20:00",
        "hidden": false
      }
    ],
    "editable": true
  }
};

export function mockDealId(record: Pick<CrmRecord, 'id'>): string {
  return `sample-moc-deal-${record.id}`;
}
export function propertyDraftKey(record: Pick<CrmRecord, 'id'>, name: string): string {
  return `${mockDealId(record)}:${name}`;
}
export function selectedPropertyValues(value: string): string[] {
  return value.split(';').filter(Boolean);
}
export function togglePropertyValue(value: string, option: string, checked: boolean): string {
  const selected = selectedPropertyValues(value);
  return (checked ? [...new Set([...selected, option])] : selected.filter(v => v !== option)).join(';');
}
