ALTER TABLE squad_sharing_wow_filters
ADD COLUMN disabled_releases INTEGER[] DEFAULT ARRAY[]::INTEGER[];