export interface NutritionData {
  food_name: string
  quantity: string
  calories: number
  protein_g: number
  carbs_g: number
  fat_g: number
  usda_fdc_id: string | null
}

export interface PersonData {
  name: string
  email: string | null
  phone: string | null
  context: string
  last_contacted: string | null
}

export interface Log {
  id: string
  created_at: string
  raw_input: string
  parsed_type: 'nutrition' | 'person'
  data: NutritionData | PersonData
}

export interface PendingLog {
  tempId: string
  raw_input: string
  failed: boolean
}

export type Entry =
  | { kind: 'log'; log: Log; justParsed: boolean }
  | { kind: 'pending'; pending: PendingLog }

export type Category = 'all' | 'nutrition' | 'person'
