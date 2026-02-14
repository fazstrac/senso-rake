/**
 * Types for the sensor mappings UI
 */

export interface Sensor {
  model: string;
  id: string;
  last_seen?: string;
  latest_ulid?: string;
}

export interface Mapping {
  mapping_id: number;
  model: string;
  id: string;
  validity_start: string; // ISO 8601 UTC timestamp
  description: string;
  deleted?: boolean;
}

export interface SensorWithMapping extends Sensor {
  mapping_id?: number;
  description?: string;
  validity_start?: string;
  deleted?: boolean;
}

export enum SensorState {
  Mapped = 'mapped',
  Unmapped = 'unmapped',
  Deleted = 'deleted',
}

export function getSensorState(sensor: SensorWithMapping): SensorState {
  if (sensor.deleted) {
    return SensorState.Deleted;
  }
  if (sensor.mapping_id) {
    return SensorState.Mapped;
  }
  return SensorState.Unmapped;
}
