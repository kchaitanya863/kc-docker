FROM python:3.11-alpine
WORKDIR /www
COPY index.html /www/index.html
EXPOSE 8080
CMD ["python3", "-m", "http.server", "8080", "--directory", "/www"]
