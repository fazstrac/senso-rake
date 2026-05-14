/**
 * API client for communicating with the SensoRake backend
 */

import type { Mapping, SensorWithMapping } from './types';

const API_BASE = process.env.API_BASE || 'http://localhost:3000';

export class ApiClient {
  private baseUrl: string;

  constructor(baseUrl: string = API_BASE) {
    this.baseUrl = baseUrl;
  }

  /**
   * Fetch all sensors and their mappings
   */
  async getSensors(): Promise<SensorWithMapping[]> {
    const response = await fetch(`${this.baseUrl}/mappings`);
    if (!response.ok) {
      throw new Error(`Failed to fetch sensors: ${response.statusText}`);
    }
    const data = await response.json();
    return Array.isArray(data) ? data : [];
  }

  /**
   * Create a new sensor mapping
   */
  async createMapping(
    model: string,
    id: string,
    validityStart: string,
    description: string
  ): Promise<Mapping> {
    const payload = {
      model,
      id,
      validity_start: validityStart,
      description,
    };

    const response = await fetch(`${this.baseUrl}/mappings`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    });

    if (!response.ok) {
      const errorText = await response.text();
      throw new Error(`Failed to create mapping: ${errorText}`);
    }

    const data = await response.json();
    return data;
  }

  /**
   * Delete (soft-delete) a mapping
   */
  async deleteMapping(mappingId: number): Promise<void> {
    const response = await fetch(`${this.baseUrl}/mappings/${mappingId}`, {
      method: 'DELETE',
    });

    if (!response.ok) {
      throw new Error(`Failed to delete mapping: ${response.statusText}`);
    }
  }

  /**
   * Restore a soft-deleted mapping
   */
  async restoreMapping(mappingId: number): Promise<void> {
    const response = await fetch(`${this.baseUrl}/mappings/${mappingId}/restore`, {
      method: 'POST',
    });

    if (!response.ok) {
      throw new Error(`Failed to restore mapping: ${response.statusText}`);
    }
  }
}

export const apiClient = new ApiClient();
