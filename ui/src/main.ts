/**
 * Main entry point for the SensoRake UI
 */

import { apiClient } from './api';
import { SensorListUI, CreateMappingFormUI } from './ui';
import './style.css';

const sensorListUI = new SensorListUI('sensor-list');
const createMappingUI = new CreateMappingFormUI('create-mapping-form');

let currentSensors: typeof import('./types').SensorWithMapping[] = [];

/**
 * Load and display all sensors
 */
async function loadSensors(): Promise<void> {
  try {
    const sensors = await apiClient.getSensors();
    currentSensors = sensors;
    sensorListUI.render(sensors);
  } catch (error) {
    console.error('Failed to load sensors:', error);
    document.getElementById('sensor-list')!.innerHTML =
      '<p class="error">Failed to load sensors. Check the console for details.</p>';
  }
}

/**
 * Handle creating a new mapping
 */
async function handleCreateMapping(e: Event): Promise<void> {
  e.preventDefault();

  const formData = createMappingUI.getFormData();
  if (!formData) {
    alert('Please fill in all required fields');
    return;
  }

  try {
    await apiClient.createMapping(
      formData.model,
      formData.id,
      formData.validity_start,
      formData.description
    );
    createMappingUI.resetForm();
    await loadSensors();
  } catch (error) {
    console.error('Failed to create mapping:', error);
    alert(`Error: ${error instanceof Error ? error.message : 'Unknown error'}`);
  }
}

/**
 * Handle deleting a mapping
 */
async function handleDeleteMapping(mappingId: number): Promise<void> {
  if (!confirm('Delete this mapping?')) {
    return;
  }

  try {
    await apiClient.deleteMapping(mappingId);
    await loadSensors();
  } catch (error) {
    console.error('Failed to delete mapping:', error);
    alert(`Error: ${error instanceof Error ? error.message : 'Unknown error'}`);
  }
}

/**
 * Handle restoring a mapping
 */
async function handleRestoreMapping(mappingId: number): Promise<void> {
  try {
    await apiClient.restoreMapping(mappingId);
    await loadSensors();
  } catch (error) {
    console.error('Failed to restore mapping:', error);
    alert(`Error: ${error instanceof Error ? error.message : 'Unknown error'}`);
  }
}

/**
 * Set up event listeners
 */
function setupEventListeners(): void {
  const createForm = document.querySelector('.create-mapping-form');
  if (createForm) {
    createForm.addEventListener('submit', handleCreateMapping);
  }

  const sensorListContainer = document.getElementById('sensor-list');
  if (sensorListContainer) {
    sensorListContainer.addEventListener('click', (e) => {
      const target = e.target as HTMLElement;

      if (target.dataset.action === 'select-sensor') {
        const model = target.dataset.model || '';
        const sensorId = target.dataset.sensorId || '';
        if (model && sensorId) {
          createMappingUI.selectSensor(model, sensorId);
          // Scroll the form into view
          const formElement = document.querySelector('.create-mapping-form');
          if (formElement) {
            formElement.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
          }
        }
      }

      if (target.dataset.action === 'delete') {
        const mappingId = parseInt(target.dataset.mappingId || '0');
        if (mappingId > 0) {
          handleDeleteMapping(mappingId);
        }
      }

      if (target.dataset.action === 'restore') {
        const mappingId = parseInt(target.dataset.mappingId || '0');
        if (mappingId > 0) {
          handleRestoreMapping(mappingId);
        }
      }
    });
  }
}

/**
 * Initialize the UI
 */
async function init(): Promise<void> {
  createMappingUI.render();
  setupEventListeners();
  await loadSensors();
}

// Start the app
init().catch((error) => {
  console.error('Failed to initialize app:', error);
});
