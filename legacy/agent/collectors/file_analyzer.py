"""
Контентный анализ файлов.

Извлекает текст из DOCX, PDF, TXT, CSV, ZIP (рекурсивно).
Используется USB-коллектором при обнаружении копирования файла.
Результат — текстовый сниппет для отправки в Policy Engine сервера.
"""
import io
import logging
import os
import zipfile

log = logging.getLogger("file_analyzer")

MAX_TEXT  = 4000   # символов — максимум текста для анализа
MAX_FSIZE = 50 * 1024 * 1024  # 50 МБ — не читать огромные файлы


def extract_text(file_path: str) -> str | None:
    """
    Извлекает текст из файла по расширению.
    Возвращает None если файл не поддерживается или слишком большой.
    """
    try:
        if not os.path.isfile(file_path):
            return None
        if os.path.getsize(file_path) > MAX_FSIZE:
            log.debug("File too large to analyze: %s", file_path)
            return None

        ext = os.path.splitext(file_path)[1].lower()

        if ext in (".txt", ".csv", ".log", ".ini", ".cfg", ".xml", ".json", ".yaml", ".yml"):
            return _read_text(file_path)
        elif ext in (".docx", ".doc"):
            return _read_docx(file_path)
        elif ext == ".pdf":
            return _read_pdf(file_path)
        elif ext in (".zip", ".docx"):   # docx — это тоже zip
            return _read_zip(file_path)
        else:
            return None
    except Exception as e:
        log.debug("extract_text(%s): %s", file_path, e)
        return None


def _read_text(path: str) -> str:
    with open(path, "r", encoding="utf-8", errors="ignore") as f:
        return f.read(MAX_TEXT)


def _read_docx(path: str) -> str:
    try:
        import docx
        doc = docx.Document(path)
        parts = []
        for para in doc.paragraphs:
            if para.text.strip():
                parts.append(para.text)
            if sum(len(p) for p in parts) >= MAX_TEXT:
                break
        # Таблицы
        for table in doc.tables:
            for row in table.rows:
                for cell in row.cells:
                    if cell.text.strip():
                        parts.append(cell.text)
                    if sum(len(p) for p in parts) >= MAX_TEXT:
                        break
        return "\n".join(parts)[:MAX_TEXT]
    except Exception as e:
        log.debug("_read_docx: %s", e)
        return ""


def _read_pdf(path: str) -> str:
    try:
        import PyPDF2
        text_parts = []
        with open(path, "rb") as f:
            reader = PyPDF2.PdfReader(f)
            for page in reader.pages[:20]:   # первые 20 страниц
                t = page.extract_text() or ""
                text_parts.append(t)
                if sum(len(p) for p in text_parts) >= MAX_TEXT:
                    break
        return "\n".join(text_parts)[:MAX_TEXT]
    except Exception as e:
        log.debug("_read_pdf: %s", e)
        return ""


def _read_zip(path: str) -> str:
    """Читает текстовые файлы внутри ZIP."""
    texts = []
    try:
        with zipfile.ZipFile(path, "r") as zf:
            for name in zf.namelist():
                ext = os.path.splitext(name)[1].lower()
                if ext not in (".txt", ".csv", ".xml", ".json", ".log"):
                    continue
                try:
                    with zf.open(name) as inner:
                        content = inner.read(MAX_TEXT).decode("utf-8", errors="ignore")
                        texts.append(content)
                except Exception:
                    pass
                if sum(len(t) for t in texts) >= MAX_TEXT:
                    break
    except Exception as e:
        log.debug("_read_zip: %s", e)
    return "\n".join(texts)[:MAX_TEXT]
