import { createContext, useContext } from 'react';
import type { CrmMetadataResponse } from '../../generated/CrmMetadataResponse';
import { MOC_DEAL_PROPERTIES } from './mocProperties';
import type { MocPropertyDefinition } from './mocProperties';

export const CrmMetadataContext = createContext<CrmMetadataResponse | null>(null);
export const useCrmMetadata = () => useContext(CrmMetadataContext);

export function metadataDealDefinitions(metadata: CrmMetadataResponse | null): Record<string, MocPropertyDefinition> {
  if (!metadata) return MOC_DEAL_PROPERTIES;
  // Once live definitions are loaded, missing properties must stay missing.
  // Do not silently substitute the bundled snapshot.
  return Object.fromEntries(metadata.properties.filter(property => property.object_type === 'deals').map(property => [property.name, {
    name: property.name, label: property.label, type: property.property_type, fieldType: property.field_type,
    options: property.options, editable: MOC_DEAL_PROPERTIES[property.name]?.editable ?? false,
  }]));
}
