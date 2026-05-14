/**
 * Unit tests for UI components
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { SensorListUI, CreateMappingFormUI } from './ui';
import { SensorState, getSensorState } from './types';
import type { SensorWithMapping } from './types';

describe('getSensorState', () => {
  it('should return Mapped for sensors with mapping_id and not deleted', () => {
    const sensor: SensorWithMapping = {
      model: 'Test',
      id: '001',
      mapping_id: 1,
      deleted: false,
    };
    expect(getSensorState(sensor)).toBe(SensorState.Mapped);
  });

  it('should return Unmapped for sensors without mapping_id', () => {
    const sensor: SensorWithMapping = {
      model: 'Test',
      id: '002',
    };
    expect(getSensorState(sensor)).toBe(SensorState.Unmapped);
  });

  it('should return Deleted for sensors with deleted flag', () => {
    const sensor: SensorWithMapping = {
      model: 'Test',
      id: '003',
      mapping_id: 1,
      deleted: true,
    };
    expect(getSensorState(sensor)).toBe(SensorState.Deleted);
  });
});

describe('SensorListUI', () => {
  let container: HTMLDivElement;

  beforeEach(() => {
    container = document.createElement('div');
    container.id = 'sensor-list';
    document.body.appendChild(container);
  });

  afterEach(() => {
    document.body.removeChild(container);
  });

  it('should render empty state when no sensors', () => {
    const ui = new SensorListUI('sensor-list');
    ui.render([]);

    expect(container.innerHTML).toContain('No sensors found');
  });

  it('should render mapped sensors section', () => {
    const sensors: SensorWithMapping[] = [
      {
        model: 'Test',
        id: '001',
        mapping_id: 1,
        description: 'Test Sensor',
        validity_start: '2025-02-14T10:00:00Z',
      },
    ];

    const ui = new SensorListUI('sensor-list');
    ui.render(sensors);

    expect(container.innerHTML).toContain('Active Mappings');
    expect(container.innerHTML).toContain('Test / 001');
    expect(container.innerHTML).toContain('Test Sensor');
  });

  it('should render unmapped sensors section', () => {
    const sensors: SensorWithMapping[] = [
      {
        model: 'Test',
        id: '002',
      },
    ];

    const ui = new SensorListUI('sensor-list');
    ui.render(sensors);

    expect(container.innerHTML).toContain('Unmapped Sensors');
    expect(container.innerHTML).toContain('Test / 002');
  });

  it('should render deleted sensors section', () => {
    const sensors: SensorWithMapping[] = [
      {
        model: 'Test',
        id: '003',
        mapping_id: 1,
        deleted: true,
        description: 'Old Sensor',
      },
    ];

    const ui = new SensorListUI('sensor-list');
    ui.render(sensors);

    expect(container.innerHTML).toContain('Deleted Mappings');
    expect(container.innerHTML).toContain('Old Sensor');
  });

  it('should include delete button for mapped sensors', () => {
    const sensors: SensorWithMapping[] = [
      {
        model: 'Test',
        id: '001',
        mapping_id: 5,
        description: 'Test',
      },
    ];

    const ui = new SensorListUI('sensor-list');
    ui.render(sensors);

    const button = container.querySelector('[data-action="delete"]');
    expect(button).toBeTruthy();
    expect(button?.getAttribute('data-mapping-id')).toBe('5');
  });

  it('should include restore button for deleted sensors', () => {
    const sensors: SensorWithMapping[] = [
      {
        model: 'Test',
        id: '001',
        mapping_id: 10,
        deleted: true,
      },
    ];

    const ui = new SensorListUI('sensor-list');
    ui.render(sensors);

    const button = container.querySelector('[data-action="restore"]');
    expect(button).toBeTruthy();
    expect(button?.getAttribute('data-mapping-id')).toBe('10');
  });
});

describe('CreateMappingFormUI', () => {
  let container: HTMLDivElement;

  beforeEach(() => {
    container = document.createElement('div');
    container.id = 'create-mapping-form';
    document.body.appendChild(container);
  });

  afterEach(() => {
    document.body.removeChild(container);
  });

  it('should render form with all required fields', () => {
    const ui = new CreateMappingFormUI('create-mapping-form');
    ui.render();

    expect(container.querySelector('#model')).toBeTruthy();
    expect(container.querySelector('#id')).toBeTruthy();
    expect(container.querySelector('#validity_start')).toBeTruthy();
    expect(container.querySelector('#description')).toBeTruthy();
    expect(container.querySelector('button[type="submit"]')).toBeTruthy();
  });

  it('should get form data when all fields are filled', () => {
    const ui = new CreateMappingFormUI('create-mapping-form');
    ui.render();

    const modelInput = container.querySelector('#model') as HTMLInputElement;
    const idInput = container.querySelector('#id') as HTMLInputElement;
    const descInput = container.querySelector('#description') as HTMLInputElement;

    modelInput.value = 'Test-Model';
    idInput.value = '001';
    descInput.value = 'Test Description';

    const data = ui.getFormData();
    expect(data).toBeTruthy();
    expect(data?.model).toBe('Test-Model');
    expect(data?.id).toBe('001');
    expect(data?.description).toBe('Test Description');
    expect(data?.validity_start).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/);
  });

  it('should return null when required fields are empty', () => {
    const ui = new CreateMappingFormUI('create-mapping-form');
    ui.render();

    const data = ui.getFormData();
    expect(data).toBeNull();
  });

  it('should reset form data', () => {
    const ui = new CreateMappingFormUI('create-mapping-form');
    ui.render();

    const modelInput = container.querySelector('#model') as HTMLInputElement;
    modelInput.value = 'Test-Model';

    ui.resetForm();

    expect(modelInput.value).toBe('');
  });

  it('should select sensor and populate form fields', () => {
    const ui = new CreateMappingFormUI('create-mapping-form');
    ui.render();

    ui.selectSensor('TempSensor', '042');

    const modelInput = container.querySelector('#model') as HTMLInputElement;
    const idInput = container.querySelector('#id') as HTMLInputElement;
    const descInput = container.querySelector('#description') as HTMLInputElement;

    expect(modelInput.value).toBe('TempSensor');
    expect(idInput.value).toBe('042');
    expect(document.activeElement).toBe(descInput);
  });
});

describe('SensorListUI - Unmapped Sensors', () => {
  let container: HTMLDivElement;

  beforeEach(() => {
    container = document.createElement('div');
    container.id = 'sensor-list';
    document.body.appendChild(container);
  });

  afterEach(() => {
    document.body.removeChild(container);
  });

  it('should make unmapped sensor names clickable', () => {
    const sensors: SensorWithMapping[] = [
      {
        model: 'TempSensor',
        id: '042',
      },
    ];

    const ui = new SensorListUI('sensor-list');
    ui.render(sensors);

    const sensorName = container.querySelector('.sensor-id--clickable');
    expect(sensorName).toBeTruthy();
    expect(sensorName?.getAttribute('data-action')).toBe('select-sensor');
    expect(sensorName?.getAttribute('data-model')).toBe('TempSensor');
    expect(sensorName?.getAttribute('data-sensor-id')).toBe('042');
  });

  it('should not make mapped sensor names clickable', () => {
    const sensors: SensorWithMapping[] = [
      {
        model: 'TempSensor',
        id: '001',
        mapping_id: 1,
      },
    ];

    const ui = new SensorListUI('sensor-list');
    ui.render(sensors);

    const sensorName = container.querySelector('.sensor-id--clickable');
    expect(sensorName).toBeFalsy();
  });
});
