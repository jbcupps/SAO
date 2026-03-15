FROM python:3.12-slim

# Azure CLI
RUN apt-get update && \
    apt-get install -y curl apt-transport-https lsb-release gnupg && \
    curl -sL https://aka.ms/InstallAzureCLIDeb | bash && \
    apt-get clean && rm -rf /var/lib/apt/lists/*

# Python deps
COPY requirements.txt /app/requirements.txt
RUN pip install --no-cache-dir -r /app/requirements.txt

# Application
COPY src/ /app/src/
COPY bicep/ /app/bicep/
COPY system_prompt.md /app/system_prompt.md

WORKDIR /app
ENV HOST_OS=windows
ENTRYPOINT ["python", "src/main.py"]
