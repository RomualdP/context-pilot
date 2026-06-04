CREATE TRIGGER trg_company_insert AFTER INSERT ON companies
BEGIN
  INSERT INTO trigger_log (event) VALUES ('INSERT company: ' || NEW.name);
END;
CREATE TRIGGER trg_company_update AFTER UPDATE ON companies
BEGIN
  INSERT INTO trigger_log (event) VALUES ('UPDATE company: ' || OLD.name || ' -> ' || NEW.name);
END;
CREATE TRIGGER trg_company_delete BEFORE DELETE ON companies
BEGIN
  INSERT INTO trigger_log (event) VALUES ('DELETE company: ' || OLD.name);
END