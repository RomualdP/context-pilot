CREATE VIEW company_summary AS
SELECT c.name, c.country, COUNT(p.id) AS employee_count
FROM companies c
LEFT JOIN people p ON p.company_id = c.id
GROUP BY c.id, c.name, c.country