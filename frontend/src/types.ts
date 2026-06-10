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

export interface AlbumData {
  artist: string
  title: string
  thoughts: string | null
  rating: number | null
  rating_tier: 'loved' | 'fine' | 'disliked' | null
  rank_position: number | null
}

export type SongStatus = 'loved' | 'to_revisit' | 'revisited'

export interface SongData {
  title: string | null
  artist: string | null
  status: SongStatus
  thoughts: string | null
  context: string | null
  source: string | null
}

export interface Log {
  id: string
  created_at: string
  raw_input: string
  parsed_type: 'nutrition' | 'person' | 'album' | 'song'
  data: NutritionData | PersonData | AlbumData | SongData
}

export type Tier = 'loved' | 'fine' | 'disliked'

export interface Opponent {
  id: string
  artist: string
  title: string
}

export interface RankComparison {
  opponent_id: string
  preferred: 'this' | 'that'
}

export type RankResponse =
  | { done: false; next_opponent: Opponent }
  | { done: true; rating: number; rank_position: number }

export interface AlbumGroups {
  loved: Log[]
  fine: Log[]
  disliked: Log[]
  unrated: Log[]
}

export interface PendingLog {
  tempId: string
  raw_input: string
  failed: boolean
}

export type Entry =
  | { kind: 'log'; log: Log; justParsed: boolean }
  | { kind: 'pending'; pending: PendingLog }

export type Category = 'all' | 'nutrition' | 'person' | 'music'
