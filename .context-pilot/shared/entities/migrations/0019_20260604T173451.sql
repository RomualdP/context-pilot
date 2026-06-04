CREATE TABLE profitable_companies AS
SELECT id, name, country, revenue
FROM companies
WHERE revenue > 1000000