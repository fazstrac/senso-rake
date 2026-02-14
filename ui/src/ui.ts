/**
 * UI components for the sensor mappings interface
 */

import type { SensorWithMapping, SensorState } from './types';
import { SensorState, getSensorState } from './types';

export class SensorListUI {
  private container: HTMLElement;

  constructor(containerId: string) {
    const el = document.getElementById(containerId);
    if (!el) {
      throw new Error(`Container ${containerId} not found`);
    }
    this.container = el;
  }

  /**
   * Render the sensors list grouped by state
   */
  render(sensors: SensorWithMapping[]): void {
    const mapped = sensors.filter((s) => getSensorState(s) === SensorState.Mapped);
    const unmapped = sensors.filter((s) => getSensorState(s) === SensorState.Unmapped);
    const deleted = sensors.filter((s) => getSensorState(s) === SensorState.Deleted);

    let html = '<div class="sensor-list">';

    if (mapped.length > 0) {
      html += this.renderSection('Active Mappings', mapped, 'mapped');
    }

    if (unmapped.length > 0) {
      html += this.renderSection('Unmapped Sensors', unmapped, 'unmapped');
    }

    if (deleted.length > 0) {
      html += this.renderSection('Deleted Mappings', deleted, 'deleted');
    }

    if (sensors.length === 0) {
      html += '<p class="empty-state">No sensors found</p>';
    }

    html += '</div>';
    this.container.innerHTML = html;
  }

  private renderSection(title: string, sensors: SensorWithMapping[], state: SensorState): string {
    let html = `<section class="sensor-section sensor-section--${state}">`;
    html += `<h2>${title}</h2>`;
    html += '<ul class="sensor-items">';

    for (const sensor of sensors) {
      html += this.renderSensorItem(sensor, state);
    }

    html += '</ul></section>';
    return html;
  }

  private renderSensorItem(sensor: SensorWithMapping, state: SensorState): string {
    const id = `${sensor.model}-${sensor.id}`;
    const isClickable = state === SensorState.Unmapped;
    const clickableAttr = isClickable ? `data-action="select-sensor" data-model="${sensor.model}" data-sensor-id="${sensor.id}"` : '';
    const clickableClass = isClickable ? ' sensor-id--clickable' : '';

    let html = `<li class="sensor-item sensor-item--${state}" data-sensor-id="${id}">`;
    html += `<div class="sensor-header">`;
    html += `<span class="sensor-id${clickableClass}" ${clickableAttr}>${sensor.model} / ${sensor.id}</span>`;

    if (sensor.description) {
      html += `<span class="sensor-description">${sensor.description}</span>`;
    }

    html += '</div>';

    if (sensor.validity_start) {
      const date = new Date(sensor.validity_start);
      html += `<div class="sensor-meta">Valid from: ${date.toLocaleDateString()} ${date.toLocaleTimeString()}</div>`;
    }

    if (state === SensorState.Mapped || state === SensorState.Deleted) {
      html += `<div class="sensor-actions">`;
      if (state === SensorState.Deleted && sensor.mapping_id) {
        html += `<button class="btn btn-restore" data-action="restore" data-mapping-id="${sensor.mapping_id}">Restore</button>`;
      } else if (state === SensorState.Mapped && sensor.mapping_id) {
        html += `<button class="btn btn-delete" data-action="delete" data-mapping-id="${sensor.mapping_id}">Delete</button>`;
      }
      html += '</div>';
    }

    html += '</li>';
    return html;
  }
}

export class CreateMappingFormUI {
  private container: HTMLElement;

  constructor(containerId: string) {
    const el = document.getElementById(containerId);
    if (!el) {
      throw new Error(`Container ${containerId} not found`);
    }
    this.container = el;
  }

  render(): void {
    const now = new Date().toISOString().slice(0, 16); // Format for datetime-local input
    const html = `
      <form class="create-mapping-form">
        <fieldset>
          <legend>Create New Mapping</legend>

          <div class="form-group">
            <label for="model">Sensor Model</label>
            <input type="text" id="model" name="model" required />
          </div>

          <div class="form-group">
            <label for="id">Sensor ID</label>
            <input type="text" id="id" name="id" required />
          </div>

          <div class="form-group">
            <label for="validity_start">Valid From (UTC)</label>
            <input type="datetime-local" id="validity_start" name="validity_start" value="${now}" required />
          </div>

          <div class="form-group">
            <label for="description">Description</label>
            <input type="text" id="description" name="description" placeholder="e.g., Living Room" required />
          </div>

          <button type="submit" class="btn btn-primary">Create Mapping</button>
        </fieldset>
      </form>
    `;
    this.container.innerHTML = html;
  }

  getFormData(): {
    model: string;
    id: string;
    validity_start: string;
    description: string;
  } | null {
    const form = this.container.querySelector('form') as HTMLFormElement | null;
    if (!form) {
      return null;
    }

    const model = (form.querySelector('#model') as HTMLInputElement).value.trim();
    const id = (form.querySelector('#id') as HTMLInputElement).value.trim();
    const validityStart = (form.querySelector('#validity_start') as HTMLInputElement).value;
    const description = (form.querySelector('#description') as HTMLInputElement).value.trim();

    if (!model || !id || !validityStart || !description) {
      return null;
    }

    // Convert local datetime to ISO 8601 UTC
    const date = new Date(validityStart);
    const isoString = date.toISOString();

    return {
      model,
      id,
      validity_start: isoString,
      description,
    };
  }

  resetForm(): void {
    const form = this.container.querySelector('form') as HTMLFormElement | null;
    if (form) {
      form.reset();
      const now = new Date().toISOString().slice(0, 16);
      (form.querySelector('#validity_start') as HTMLInputElement).value = now;
    }
  }

  /**
   * Pre-fill form with a sensor's model and id, then focus on description
   */
  selectSensor(model: string, sensorId: string): void {
    const form = this.container.querySelector('form') as HTMLFormElement | null;
    if (form) {
      (form.querySelector('#model') as HTMLInputElement).value = model;
      (form.querySelector('#id') as HTMLInputElement).value = sensorId;
      const descInput = form.querySelector('#description') as HTMLInputElement;
      descInput.focus();
      descInput.select();
    }
  }
}
