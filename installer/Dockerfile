FROM mcr.microsoft.com/azure-cli:latest

RUN tdnf install -y python3-pip && \
    ln -sf /usr/bin/python3.12 /usr/bin/python && \
    tdnf clean all

# Python deps
COPY requirements.txt /app/requirements.txt
RUN python -m pip install --no-cache-dir -r /app/requirements.txt

# Application
COPY src/ /app/src/
COPY bicep/ /app/bicep/
COPY shared/ /app/shared/
COPY system_prompt.md /app/system_prompt.md

WORKDIR /app
ENV HOST_OS=windows
ENTRYPOINT ["python", "src/main.py"]
