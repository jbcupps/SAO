FROM mcr.microsoft.com/azure-cli:2.69.0

ENV PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1 \
    VIRTUAL_ENV=/opt/venv \
    PATH=/opt/venv/bin:$PATH \
    AZURE_CONFIG_DIR=/home/sao/.azure \
    HOST_OS=windows

RUN python3 -m venv "$VIRTUAL_ENV" \
    && "$VIRTUAL_ENV/bin/pip" install --no-cache-dir --upgrade pip \
    && useradd --system --create-home --uid 10001 --shell /usr/sbin/nologin sao \
    && mkdir -p /app /home/sao/.azure \
    && chown -R sao:sao /app /home/sao

COPY requirements.txt /app/requirements.txt
RUN "$VIRTUAL_ENV/bin/pip" install --no-cache-dir -r /app/requirements.txt

COPY src/ /app/src/
COPY bicep/ /app/bicep/
COPY shared/ /app/shared/
COPY system_prompt.md /app/system_prompt.md

WORKDIR /app
USER sao
ENTRYPOINT ["python", "src/main.py"]
