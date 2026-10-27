import type { DeepReadonly } from 'vue'
import type { Page } from '~/types/engine'

export interface PageNodeData { page: DeepReadonly<Page>, home: boolean, onDock: boolean }
