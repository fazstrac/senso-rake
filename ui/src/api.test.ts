/**
 * Unit tests for the API client
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { ApiClient } from './api';

describe('ApiClient', () => {
  let apiClient: ApiClient;

  beforeEach(() => {
    apiClient = new ApiClient('http://localhost:3000');
    global.fetch = vi.fn();
  });

  describe('getSensors', () => {
    it('should fetch sensors from the API', async () => {
      const mockSensors = [
        {
          model: 'Test-Model',
          id: '001',
          mapping_id: 1,
          description: 'Test Sensor',
          validity_start: '2025-02-14T10:00:00Z',
        },
      ];

      global.fetch = vi.fn(() =>
        Promise.resolve({
          ok: true,
          json: () => Promise.resolve(mockSensors),
        } as Response)
      );

      const sensors = await apiClient.getSensors();
      expect(sensors).toEqual(mockSensors);
      expect(global.fetch).toHaveBeenCalledWith('http://localhost:3000/mappings');
    });

    it('should return empty array if response is not OK', async () => {
      global.fetch = vi.fn(() =>
        Promise.resolve({
          ok: false,
          statusText: 'Not Found',
        } as Response)
      );

      try {
        await apiClient.getSensors();
        expect.fail('Should have thrown an error');
      } catch (error) {
        expect((error as Error).message).toContain('Failed to fetch sensors');
      }
    });

    it('should handle non-array responses', async () => {
      global.fetch = vi.fn(() =>
        Promise.resolve({
          ok: true,
          json: () => Promise.resolve({ error: 'Invalid' }),
        } as Response)
      );

      const sensors = await apiClient.getSensors();
      expect(sensors).toEqual([]);
    });
  });

  describe('createMapping', () => {
    it('should create a new mapping', async () => {
      const mockResponse = {
        mapping_id: 42,
        model: 'Test-Model',
        id: '001',
        description: 'Test Sensor',
        validity_start: '2025-02-14T10:00:00Z',
      };

      global.fetch = vi.fn(() =>
        Promise.resolve({
          ok: true,
          json: () => Promise.resolve(mockResponse),
        } as Response)
      );

      const result = await apiClient.createMapping(
        'Test-Model',
        '001',
        '2025-02-14T10:00:00Z',
        'Test Sensor'
      );

      expect(result).toEqual(mockResponse);
      expect(global.fetch).toHaveBeenCalledWith(
        'http://localhost:3000/mappings',
        expect.objectContaining({
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
        })
      );
    });

    it('should throw error if creation fails', async () => {
      global.fetch = vi.fn(() =>
        Promise.resolve({
          ok: false,
          statusText: 'Conflict',
          text: () => Promise.resolve('Mapping already exists'),
        } as Response)
      );

      try {
        await apiClient.createMapping('Test', '001', '2025-02-14T10:00:00Z', 'Test');
        expect.fail('Should have thrown an error');
      } catch (error) {
        expect((error as Error).message).toContain('Failed to create mapping');
      }
    });
  });

  describe('deleteMapping', () => {
    it('should delete a mapping', async () => {
      global.fetch = vi.fn(() =>
        Promise.resolve({
          ok: true,
        } as Response)
      );

      await apiClient.deleteMapping(42);
      expect(global.fetch).toHaveBeenCalledWith('http://localhost:3000/mappings/42', {
        method: 'DELETE',
      });
    });

    it('should throw error if deletion fails', async () => {
      global.fetch = vi.fn(() =>
        Promise.resolve({
          ok: false,
          statusText: 'Internal Server Error',
        } as Response)
      );

      try {
        await apiClient.deleteMapping(42);
        expect.fail('Should have thrown an error');
      } catch (error) {
        expect((error as Error).message).toContain('Failed to delete mapping');
      }
    });
  });

  describe('restoreMapping', () => {
    it('should restore a mapping', async () => {
      global.fetch = vi.fn(() =>
        Promise.resolve({
          ok: true,
        } as Response)
      );

      await apiClient.restoreMapping(42);
      expect(global.fetch).toHaveBeenCalledWith('http://localhost:3000/mappings/42/restore', {
        method: 'POST',
      });
    });

    it('should throw error if restore fails', async () => {
      global.fetch = vi.fn(() =>
        Promise.resolve({
          ok: false,
          statusText: 'Not Found',
        } as Response)
      );

      try {
        await apiClient.restoreMapping(42);
        expect.fail('Should have thrown an error');
      } catch (error) {
        expect((error as Error).message).toContain('Failed to restore mapping');
      }
    });
  });
});
